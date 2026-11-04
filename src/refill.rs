//! Refill the Bitrefill balance from shielded ZEC (Bitrefill lane 5) via NEAR Intents 1Click.
//! Bitrefill doesn't take ZEC, so 1Click swaps it: we ask for a quote that delivers exactly
//! `usd` of USDC/BTC to the Bitrefill deposit address, and 1Click gives us a Zcash deposit
//! address + exact ZEC amount. The operator sends that from any shielded wallet.
//! Mainnet only (1Click has no testnet). Deshields that one transfer: say so in PRIVACY.md.

use std::time::Duration;

use serde_json::{Value, json};

const BASE: &str = "https://1click.chaindefuser.com/v0";
const ZEC_ASSET: &str = "nep141:zec.omft.near";
const SLIPPAGE_BPS: i64 = 100;
const QUOTE_TTL_MINS: i64 = 30;
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

/// Where the swapped funds land: Bitrefill "Add funds" offers both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dest {
    UsdcBase,
    Btc,
}

impl Dest {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "usdc-base" => Some(Self::UsdcBase),
            "btc" => Some(Self::Btc),
            _ => None,
        }
    }

    fn asset(self) -> &'static str {
        match self {
            Self::UsdcBase => "nep141:base-0x833589fcd6edb6e08f4c7c32d4f71b54bda02913.omft.near",
            Self::Btc => "nep141:btc.omft.near",
        }
    }

    /// Output amount in the asset's smallest unit for `usd_cents` (BTC needs the BTC price).
    fn amount_out(self, usd_cents: i64, btc_usd_cents: i64) -> Option<i64> {
        match self {
            Self::UsdcBase => usd_cents.checked_mul(10_000), // 6 decimals
            Self::Btc => {
                if btc_usd_cents <= 0 {
                    return None;
                }
                let sats = (usd_cents as i128 * 100_000_000).div_euclid(btc_usd_cents as i128);
                i64::try_from(sats).ok().filter(|s| *s > 0)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefillQuote {
    /// Send ZEC here (transparent address run by 1Click). `None` on dry quotes.
    pub deposit_address: Option<String>,
    pub amount_in_zat: i64,
    pub amount_in_formatted: String,
    pub amount_out_formatted: String,
    pub time_estimate_secs: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwapStatus {
    PendingDeposit,
    KnownDepositTx,
    IncompleteDeposit,
    Processing,
    Success,
    Refunded,
    Failed,
    Unknown(String),
}

#[allow(clippy::too_many_arguments)]
pub fn quote_request(
    dest: Dest,
    usd_cents: i64,
    btc_usd_cents: i64,
    recipient: &str,
    refund_to: &str,
    deadline_rfc3339: &str,
    dry: bool,
) -> anyhow::Result<Value> {
    anyhow::ensure!(usd_cents > 0, "amount must be positive");
    anyhow::ensure!(
        !recipient.trim().is_empty(),
        "Bitrefill deposit address is empty"
    );
    anyhow::ensure!(
        refund_to.starts_with('t') || refund_to.starts_with('u'),
        "refund address must be a Zcash address"
    );
    let amount = dest
        .amount_out(usd_cents, btc_usd_cents)
        .ok_or_else(|| anyhow::anyhow!("amount out of range"))?;
    Ok(json!({
        "dry": dry,
        "swapType": "EXACT_OUTPUT",
        "slippageTolerance": SLIPPAGE_BPS,
        "originAsset": ZEC_ASSET,
        "depositType": "ORIGIN_CHAIN",
        "destinationAsset": dest.asset(),
        "amount": amount.to_string(),
        "refundTo": refund_to,
        "refundType": "ORIGIN_CHAIN",
        "recipient": recipient,
        "recipientType": "DESTINATION_CHAIN",
        "deadline": deadline_rfc3339,
    }))
}

pub fn parse_quote(json: &str) -> anyhow::Result<RefillQuote> {
    let v: Value = serde_json::from_str(json)?;
    let q = v
        .get("quote")
        .ok_or_else(|| anyhow::anyhow!("1Click: {}", error_text(&v)))?;
    let s = |k: &str| q.get(k).and_then(Value::as_str).map(str::to_owned);
    let amount_in_zat = s("amountIn")
        .and_then(|a| a.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("1Click quote has no amountIn"))?;
    Ok(RefillQuote {
        deposit_address: s("depositAddress"),
        amount_in_zat,
        amount_in_formatted: s("amountInFormatted").unwrap_or_default(),
        amount_out_formatted: s("amountOutFormatted").unwrap_or_default(),
        time_estimate_secs: q.get("timeEstimate").and_then(Value::as_i64).unwrap_or(0),
    })
}

pub fn parse_status(json: &str) -> anyhow::Result<SwapStatus> {
    let v: Value = serde_json::from_str(json)?;
    let status = v
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("1Click: {}", error_text(&v)))?;
    Ok(match status {
        "PENDING_DEPOSIT" => SwapStatus::PendingDeposit,
        "KNOWN_DEPOSIT_TX" => SwapStatus::KnownDepositTx,
        "INCOMPLETE_DEPOSIT" => SwapStatus::IncompleteDeposit,
        "PROCESSING" => SwapStatus::Processing,
        "SUCCESS" => SwapStatus::Success,
        "REFUNDED" => SwapStatus::Refunded,
        "FAILED" => SwapStatus::Failed,
        other => SwapStatus::Unknown(other.to_owned()),
    })
}

fn error_text(v: &Value) -> String {
    v.get("message")
        .and_then(Value::as_str)
        .unwrap_or("unexpected response")
        .chars()
        .take(200)
        .collect()
}

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(HTTP_TIMEOUT).build()?)
}

/// Live quote. `dry=false` reserves a deposit address valid for `QUOTE_TTL_MINS`.
pub async fn quote(
    dest: Dest,
    usd_cents: i64,
    btc_usd_cents: i64,
    recipient: &str,
    refund_to: &str,
    dry: bool,
) -> anyhow::Result<RefillQuote> {
    let deadline = (chrono::Utc::now() + chrono::Duration::minutes(QUOTE_TTL_MINS))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let body = quote_request(
        dest,
        usd_cents,
        btc_usd_cents,
        recipient,
        refund_to,
        &deadline,
        dry,
    )?;
    let text = client()?
        .post(format!("{BASE}/quote"))
        .json(&body)
        .send()
        .await?
        .text()
        .await?;
    parse_quote(&text)
}

pub async fn status(deposit_address: &str) -> anyhow::Result<SwapStatus> {
    let text = client()?
        .get(format!("{BASE}/status"))
        .query(&[("depositAddress", deposit_address)])
        .send()
        .await?
        .text()
        .await?;
    parse_status(&text)
}

/// `zcash:` URI with the exact amount, so a phone wallet can scan it (ZIP-321).
pub fn payment_uri(q: &RefillQuote) -> Option<String> {
    let addr = q.deposit_address.as_deref()?;
    crate::zip321::deposit_uri(addr, Some(q.amount_in_zat), None).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRY: &str = include_str!("../fixtures/refill/quote-dry.json");

    #[test]
    fn parses_live_dry_quote() {
        let q = parse_quote(DRY).unwrap();
        assert!(
            q.amount_in_zat > 100_000 && q.amount_in_zat < 1_000_000,
            "{q:?}"
        );
        assert_eq!(q.amount_out_formatted, "5.0");
        assert_eq!(q.deposit_address, None, "dry quotes reserve no address");
        assert!(q.time_estimate_secs > 0);
    }

    #[test]
    fn request_asks_exact_output_in_smallest_units() {
        let req = quote_request(
            Dest::UsdcBase,
            500,
            7_838_800,
            "0xabc",
            "t1refund",
            "2026-11-04T11:00:00Z",
            true,
        )
        .unwrap();
        assert_eq!(req["amount"], "5000000");
        assert_eq!(req["swapType"], "EXACT_OUTPUT");
        assert_eq!(req["originAsset"], ZEC_ASSET);
        let btc = quote_request(Dest::Btc, 500, 7_838_800, "bc1x", "t1refund", "d", true).unwrap();
        assert_eq!(btc["amount"], "6378", "$5 at $78,388/BTC");
    }

    #[test]
    fn request_rejects_bad_inputs() {
        assert!(quote_request(Dest::UsdcBase, 0, 1, "0xabc", "t1r", "d", true).is_err());
        assert!(quote_request(Dest::UsdcBase, 500, 1, " ", "t1r", "d", true).is_err());
        assert!(quote_request(Dest::UsdcBase, 500, 1, "0xabc", "0xnotzec", "d", true).is_err());
        assert!(quote_request(Dest::Btc, 500, 0, "bc1x", "t1r", "d", true).is_err());
    }

    #[test]
    fn status_maps_every_documented_state() {
        for (s, want) in [
            ("PENDING_DEPOSIT", SwapStatus::PendingDeposit),
            ("PROCESSING", SwapStatus::Processing),
            ("SUCCESS", SwapStatus::Success),
            ("REFUNDED", SwapStatus::Refunded),
            ("FAILED", SwapStatus::Failed),
        ] {
            assert_eq!(
                parse_status(&format!(r#"{{"status":"{s}"}}"#)).unwrap(),
                want
            );
        }
        assert_eq!(
            parse_status(r#"{"status":"NEW"}"#).unwrap(),
            SwapStatus::Unknown("NEW".into())
        );
        assert!(
            parse_status(r#"{"message":"Deposit address x not found","statusCode":404}"#).is_err()
        );
    }

    #[test]
    fn payment_uri_has_exact_amount() {
        let q = RefillQuote {
            deposit_address: Some("t1Dep".into()),
            amount_in_zat: 388_152,
            amount_in_formatted: "0.00388152".into(),
            amount_out_formatted: "5.0".into(),
            time_estimate_secs: 459,
        };
        assert_eq!(
            payment_uri(&q).as_deref(),
            Some("zcash:t1Dep?amount=0.00388152")
        );
    }
}
