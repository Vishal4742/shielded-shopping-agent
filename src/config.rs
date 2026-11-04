//! Configuration from `.env` (docs/05 §8). Secrets are redacted in `Debug`; never log them.

use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

const DEFAULT_DEVTOOL: &str = "tools/zcash-devtool/target/release/zcash-devtool";
const DEFAULT_STATS_ADDR: &str = "127.0.0.1:8787";
const PAUSED_KEY: &str = "SSA_PAUSED";

pub struct Config {
    pub telegram_bot_token: String,
    pub bitrefill_api_key: String,
    pub anthropic_api_key: String,
    pub devtool: PathBuf,
    pub treasury_ua: String,
    /// `BITREFILL_DRY_RUN`; defaults to true (safe) when unset.
    pub bitrefill_dry_run: bool,
    /// `None` when `SSA_STATS_ADDR` is set but empty (no stats server).
    pub stats_addr: Option<SocketAddr>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("telegram_bot_token", &"<redacted>")
            .field("bitrefill_api_key", &"<redacted>")
            .field("anthropic_api_key", &"<redacted>")
            .field("devtool", &self.devtool)
            .field("treasury_ua", &self.treasury_ua)
            .field("bitrefill_dry_run", &self.bitrefill_dry_run)
            .field("stats_addr", &self.stats_addr)
            .finish()
    }
}

impl Config {
    /// Loads `.env` into the process env (if present), then parses. Required keys: the three
    /// secrets and `SSA_TREASURY_UA`; a missing one fails with its name.
    pub fn from_env() -> Result<Config> {
        dotenvy::dotenv().ok();
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Parses from any key lookup (tests pass a closure over a map). Empty values count as unset,
    /// except `SSA_STATS_ADDR` where empty means "no stats server".
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Config> {
        let val = |k: &str| {
            get(k)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let required = |k: &str| val(k).ok_or_else(|| anyhow!("missing required config key {k}"));
        let or_default = |k: &str, d: &str| val(k).unwrap_or_else(|| d.to_owned());

        let bitrefill_dry_run = match val("BITREFILL_DRY_RUN").as_deref() {
            None | Some("1") => true,
            Some("0") => false,
            Some(_) => bail!("BITREFILL_DRY_RUN must be 0 or 1"),
        };
        let stats_addr = match get("SSA_STATS_ADDR").map(|v| v.trim().to_owned()) {
            None => Some(DEFAULT_STATS_ADDR.to_owned()),
            Some(v) if v.is_empty() => None,
            Some(v) => Some(v),
        }
        .map(|v| {
            v.parse::<SocketAddr>()
                .context("SSA_STATS_ADDR must be host:port")
        })
        .transpose()?;

        Ok(Config {
            telegram_bot_token: required("TELEGRAM_BOT_TOKEN")?,
            bitrefill_api_key: required("BITREFILL_API_KEY")?,
            anthropic_api_key: required("ANTHROPIC_API_KEY")?,
            devtool: PathBuf::from(or_default("SSA_DEVTOOL", DEFAULT_DEVTOOL)),
            treasury_ua: required("SSA_TREASURY_UA")?,
            bitrefill_dry_run,
            stats_addr,
        })
    }
}

/// Kill switch. Re-reads `SSA_PAUSED` from the `.env` file on every call (the process env is only
/// filled once at boot). `"1"` means paused. A missing file, missing key or parse error means
/// not paused. Nothing is logged, so nothing secret can leak.
pub fn paused(env_path: &Path) -> bool {
    let Ok(iter) = dotenvy::from_path_iter(env_path) else {
        return false;
    };
    iter.map_while(Result::ok)
        .find(|(k, _)| k == PAUSED_KEY)
        .is_some_and(|(_, v)| v.trim() == "1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn base() -> HashMap<&'static str, &'static str> {
        HashMap::from([
            ("TELEGRAM_BOT_TOKEN", "tg-secret-123"),
            ("BITREFILL_API_KEY", "br-secret-456"),
            ("ANTHROPIC_API_KEY", "an-secret-789"),
            ("SSA_TREASURY_UA", "utest1treasury"),
        ])
    }

    fn parse(m: &HashMap<&str, &str>) -> Result<Config> {
        Config::from_lookup(|k| m.get(k).map(|v| (*v).to_owned()))
    }

    #[test]
    fn defaults_apply_when_only_required_keys_set() {
        let c = parse(&base()).unwrap();
        assert_eq!(c.devtool, PathBuf::from(DEFAULT_DEVTOOL));
        assert!(c.bitrefill_dry_run);
        assert_eq!(c.stats_addr, Some("127.0.0.1:8787".parse().unwrap()));
    }

    #[test]
    fn each_missing_required_key_is_named_in_the_error() {
        for key in [
            "TELEGRAM_BOT_TOKEN",
            "BITREFILL_API_KEY",
            "ANTHROPIC_API_KEY",
            "SSA_TREASURY_UA",
        ] {
            let mut m = base();
            m.remove(key);
            let err = parse(&m).unwrap_err().to_string();
            assert!(err.contains(key), "{err}");
        }
    }

    #[test]
    fn blank_required_value_counts_as_missing() {
        let mut m = base();
        m.insert("BITREFILL_API_KEY", "  ");
        assert!(
            parse(&m)
                .unwrap_err()
                .to_string()
                .contains("BITREFILL_API_KEY")
        );
    }

    #[test]
    fn debug_redacts_secrets() {
        let dbg = format!("{:?}", parse(&base()).unwrap());
        for secret in ["tg-secret-123", "br-secret-456", "an-secret-789"] {
            assert!(!dbg.contains(secret), "{dbg}");
        }
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn optional_keys_parse_and_reject_garbage() {
        let mut m = base();
        m.insert("BITREFILL_DRY_RUN", "0");
        m.insert("SSA_STATS_ADDR", "");
        let c = parse(&m).unwrap();
        assert!(!c.bitrefill_dry_run);
        assert_eq!(c.stats_addr, None);

        for (k, v) in [("BITREFILL_DRY_RUN", "yes"), ("SSA_STATS_ADDR", "nope")] {
            let mut m = base();
            m.insert(k, v);
            assert!(parse(&m).unwrap_err().to_string().contains(k));
        }
    }

    fn temp_env(contents: Option<&str>) -> PathBuf {
        let p = std::env::temp_dir().join(format!("ssa-env-{}", uuid::Uuid::new_v4()));
        if let Some(c) = contents {
            std::fs::write(&p, c).unwrap();
        }
        p
    }

    #[test]
    fn paused_rereads_the_file_on_every_call() {
        let p = temp_env(Some("SSA_PAUSED=0\n"));
        assert!(!paused(&p));
        std::fs::write(&p, "OTHER=x\nSSA_PAUSED=1\n").unwrap();
        assert!(paused(&p));
        std::fs::write(&p, "SSA_PAUSED=0\n").unwrap();
        assert!(!paused(&p));
        std::fs::remove_file(&p).unwrap();
    }

    #[test]
    fn paused_is_false_for_missing_file_missing_key_or_garbage() {
        assert!(!paused(&temp_env(None)));
        let p = temp_env(Some("FOO=bar\n"));
        assert!(!paused(&p));
        std::fs::write(&p, "SSA_PAUSED=maybe\n").unwrap();
        assert!(!paused(&p));
        std::fs::write(&p, "\u{0}\u{1}=== not an env file").unwrap();
        assert!(!paused(&p));
        std::fs::remove_file(&p).unwrap();
    }
}
