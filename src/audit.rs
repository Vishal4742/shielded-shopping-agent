//! Selective-disclosure audit: an auditor holding ONLY a user's UFVK rebuilds that user's purchase
//! list from the chain. Flow (verified against devtool `0b3436b`, notes/devtool.md "Watch-only
//! audit"): `init-fvk` -> `sync` -> `enhance` -> `list-tx`. `enhance` is required: after `sync`
//! alone the outgoing receipt memo is missing. The temp wallet is view-only (no identity file, no
//! spend ability) and is deleted afterwards.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use tokio::process::Command;

use crate::zec::{self, Tx};

/// First sync scans from the birthday, which can take minutes on a long range.
const STEP_TIMEOUT: Duration = Duration::from_secs(600);
const STDERR_TAIL: usize = 300;
const UFVK_PREFIX: &str = "uview";
const MEMO_TAG: &str = "SSA";
const MEMO_SEP: char = '·';
const RUPEE: char = '₹';
pub const CSV_HEADER: &str = "txid,mined_height,order_ref,operator,inr,zat";

/// One purchase receipt found in a user's chain history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditLine {
    pub txid: String,
    pub mined_height: Option<i64>,
    pub order_ref: String,
    pub operator: String,
    pub inr: i64,
    pub zat: i64,
}

/// `SSA·<id>·<op>·₹<n>` -> `(id, op, n)`. Id and operator are limited to `[A-Za-z0-9_-]`, which
/// keeps the CSV free of quoting and of spreadsheet-formula starts (`=`, `+`, `@`).
fn parse_memo(memo: &str) -> Option<(&str, &str, i64)> {
    let mut parts = memo.split(MEMO_SEP);
    let (tag, id, op, amount) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || tag != MEMO_TAG {
        return None;
    }
    let token_ok = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    };
    let digits = amount.strip_prefix(RUPEE)?;
    let digits_ok = !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit());
    if !token_ok(id) || !token_ok(op) || !digits_ok {
        return None;
    }
    Some((id, op, digits.parse().ok()?))
}

/// Receipt lines from parsed `list-tx` output: every tx with an `SSA·…` memo, once per txid
/// (devtool prints a tx once per wallet account), in first-seen order. Everything else is ignored.
// ponytail: memos are unauthenticated and `TxOutput` has no sent_from, so an `SSA·` memo that
// someone else sends the user would also be listed. Add sent_from to zec::TxOutput and require it
// if reports are ever used as proof of purchase.
pub fn receipts_from(txs: &[Tx]) -> Vec<AuditLine> {
    let mut seen: HashSet<&str> = HashSet::new();
    txs.iter()
        .filter_map(|tx| {
            let (output, (id, op, inr)) = tx
                .outputs
                .iter()
                .find_map(|o| Some((o, parse_memo(o.memo.as_deref()?)?)))?;
            seen.insert(&tx.txid).then(|| AuditLine {
                txid: tx.txid.clone(),
                mined_height: tx.mined_height,
                order_ref: id.to_owned(),
                operator: op.to_owned(),
                inr,
                zat: output.value_zat,
            })
        })
        .collect()
}

pub fn to_csv(lines: &[AuditLine]) -> String {
    let mut out = format!("{CSV_HEADER}\n");
    for l in lines {
        let height = l.mined_height.map(|h| h.to_string()).unwrap_or_default();
        let _ = writeln!(
            out,
            "{},{},{},{},{},{}",
            l.txid, height, l.order_ref, l.operator, l.inr, l.zat
        );
    }
    out
}

/// Private temp dir (0700) that is removed on drop, including on error or task cancellation.
struct TempWallet(PathBuf);

impl TempWallet {
    fn create() -> std::io::Result<Self> {
        let dir = std::env::temp_dir().join(format!("ssa-audit-{}", uuid::Uuid::new_v4()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for TempWallet {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Run `devtool wallet -w <dir> <args…>`. `secret` is scrubbed from any error text.
async fn run(devtool: &Path, dir: &Path, args: &[&str], secret: &str) -> anyhow::Result<String> {
    let mut cmd = Command::new(devtool);
    cmd.arg("wallet")
        .arg("-w")
        .arg(dir)
        .args(args)
        .kill_on_drop(true);
    let step = args.first().copied().unwrap_or("?");
    let out = tokio::time::timeout(STEP_TIMEOUT, cmd.output())
        .await
        .with_context(|| format!("devtool {step} timed out after {}s", STEP_TIMEOUT.as_secs()))?
        .with_context(|| format!("could not start devtool {step}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).replace(secret, "<ufvk>");
        let tail: String = stderr.trim().chars().rev().take(STDERR_TAIL).collect();
        bail!(
            "devtool {step} failed ({:?}): {}",
            out.status.code(),
            tail.chars().rev().collect::<String>()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Rebuild one user's purchase list from their UFVK alone. `birthday` is the account's birthday
/// height (the wallet scans from there). Never logs the UFVK.
// ponytail: the UFVK is a process argument, so it is visible in `ps` on the auditor's machine for
// the duration of init-fvk; devtool has no stdin/file input for it. Fine for a hackathon laptop.
// ponytail: direct connection, so the light-wallet server sees the sync pattern; add
// `--connection tor` if auditors need that privacy.
pub async fn audit(devtool: &Path, ufvk: &str, birthday: i64) -> anyhow::Result<Vec<AuditLine>> {
    if !ufvk.starts_with(UFVK_PREFIX) {
        bail!("not a testnet UFVK (expected it to start with {UFVK_PREFIX:?})");
    }
    if birthday < 0 {
        bail!("birthday must be a block height, got {birthday}");
    }
    let wallet = TempWallet::create().context("could not create temp wallet dir")?;
    let dir = wallet.0.as_path();
    let birthday = birthday.to_string();
    let init = [
        "init-fvk",
        "--name",
        "audit",
        "--fvk",
        ufvk,
        "--birthday",
        &birthday,
    ];
    run(devtool, dir, &init, ufvk).await?;
    run(devtool, dir, &["sync"], ufvk).await?;
    run(devtool, dir, &["enhance"], ufvk).await?;
    let text = run(devtool, dir, &["list-tx"], ufvk).await?;
    let txs = zec::parse_list_tx(&text).context("could not parse devtool list-tx output")?;
    Ok(receipts_from(&txs))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_WALLET: &str = include_str!("../fixtures/devtool/list-tx-memo.txt");
    const WATCH_ONLY: &str = include_str!("../fixtures/devtool/list-tx-watchonly.txt");
    const MEMO_TXID: &str = "5c5abb6f8c0fe605c9f923ab160e13769dd70a4c963ff10bb425a816c811bf5a";

    fn expected_line() -> AuditLine {
        AuditLine {
            txid: MEMO_TXID.to_owned(),
            mined_height: Some(4_581_535),
            order_ref: "00000000-test-memo".to_owned(),
            operator: "jio".to_owned(),
            inr: 10,
            zat: 100_000,
        }
    }

    #[test]
    fn full_wallet_listing_yields_one_deduped_receipt() {
        // memo tx is printed once per account (twice); faucet receipts have no memo
        let txs = zec::parse_list_tx(FULL_WALLET).unwrap();
        assert_eq!(receipts_from(&txs), vec![expected_line()]);
    }

    #[test]
    fn watch_only_listing_sees_the_receipt_and_not_the_treasury_faucet_tx() {
        let txs = zec::parse_list_tx(WATCH_ONLY).unwrap();
        assert!(txs.iter().all(|t| !t.txid.starts_with("3a999d11")));
        // the user's own faucet memo ("ssa faucet test") is not a receipt
        assert_eq!(receipts_from(&txs), vec![expected_line()]);
    }

    #[test]
    fn memo_parser_accepts_only_the_receipt_format() {
        assert_eq!(
            parse_memo("SSA·abc-1·airtel·₹50"),
            Some(("abc-1", "airtel", 50))
        );
        for bad in [
            "",
            "ssa faucet test",
            "SSA·id·jio",
            "SSA·id·jio·₹10·extra",
            "SSA·id·jio·10",
            "SSA·id·jio·₹",
            "SSA·id·jio·₹-5",
            "SSA·id·jio·₹1.5",
            "SSA··jio·₹10",
            "SSA·id··₹10",
            "XYZ·id·jio·₹10",
            "SSA·=cmd,x·jio·₹10",
            "SSA·id·jio·₹99999999999999999999",
        ] {
            assert_eq!(parse_memo(bad), None, "{bad:?} should be ignored");
        }
    }

    #[test]
    fn unmined_receipt_has_no_height_and_csv_leaves_it_empty() {
        let text = format!(
            "Transactions:\n{MEMO_TXID}\n  Output 0 (Ironwood)\n    Value:   0.001 TAZ\n    Memo: Memo::Text(\"SSA·o1·vi·₹20\")\n"
        );
        let lines = receipts_from(&zec::parse_list_tx(&text).unwrap());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].mined_height, None);
        assert_eq!(
            to_csv(&lines),
            format!("{CSV_HEADER}\n{MEMO_TXID},,o1,vi,20,100000\n")
        );
    }

    #[test]
    fn empty_input_gives_no_receipts() {
        assert!(receipts_from(&[]).is_empty());
    }

    #[test]
    fn csv_has_header_and_one_row_per_line() {
        let csv = to_csv(&[expected_line()]);
        assert_eq!(
            csv,
            format!(
                "txid,mined_height,order_ref,operator,inr,zat\n{MEMO_TXID},4581535,00000000-test-memo,jio,10,100000\n"
            )
        );
        assert_eq!(
            to_csv(&[]),
            "txid,mined_height,order_ref,operator,inr,zat\n"
        );
    }

    #[tokio::test]
    async fn rejects_non_ufvk_and_missing_binary_without_echoing_the_key() {
        let bin = Path::new("/nonexistent/devtool");
        let err = audit(bin, "-oops", 1).await.unwrap_err();
        assert!(err.to_string().contains("not a testnet UFVK"));
        let err = audit(bin, "uviewtest1fake", 1).await.unwrap_err();
        assert!(err.to_string().contains("could not start devtool init-fvk"));
        assert!(!format!("{err:#}").contains("uviewtest1fake"));
    }
}
