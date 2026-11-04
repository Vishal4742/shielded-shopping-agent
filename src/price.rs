use serde_json::value::RawValue;
use thiserror::Error;
use tokio::sync::Mutex;

const TOKENS_URL: &str = "https://1click.chaindefuser.com/v0/tokens";

#[derive(Debug, Error)]
pub enum PriceError {
    #[error("invalid token price response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZEC price missing, invalid, or out of range")]
    InvalidPrice,
    #[error("price request failed: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(serde::Deserialize)]
struct Token<'a> {
    #[serde(rename = "assetId", borrow)]
    asset_id: &'a str,
    #[serde(borrow)]
    price: &'a RawValue,
}

pub const ZEC_ASSET: &str = "nep141:zec.omft.near";
/// Bitrefill prices our account in BTC (satoshis), so quotes need BTC/USD too.
pub const BTC_ASSET: &str = "nep141:btc.omft.near";
const CACHE_SECS: i64 = 60;

pub fn parse_usd_cents(json: &str, asset_id: &str) -> Result<i64, PriceError> {
    let tokens: Vec<Token<'_>> = serde_json::from_str(json)?;
    let raw = tokens
        .iter()
        .find(|token| token.asset_id == asset_id)
        .ok_or(PriceError::InvalidPrice)?
        .price
        .get();
    decimal_cents(raw)
}

fn decimal_cents(raw: &str) -> Result<i64, PriceError> {
    // Plain decimals only ("1287.77"); 1Click never sends exponents or signs.
    let (whole, frac) = raw.split_once('.').unwrap_or((raw, ""));
    let digits_only = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || !digits_only(whole) || !digits_only(frac) {
        return Err(PriceError::InvalidPrice);
    }
    let n: i128 = format!("{whole}{frac}")
        .parse()
        .map_err(|_| PriceError::InvalidPrice)?;
    let scale = 2 - frac.len() as i32;
    let cents = if scale >= 0 {
        n.checked_mul(10_i128.pow(scale as u32))
    } else {
        let divisor = 10_i128
            .checked_pow((-scale) as u32)
            .ok_or(PriceError::InvalidPrice)?;
        Some((n + divisor / 2) / divisor)
    }
    .ok_or(PriceError::InvalidPrice)?;
    i64::try_from(cents).map_err(|_| PriceError::InvalidPrice)
}

/// USD cents per whole coin, and when they were fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prices {
    pub zec_usd_cents: i64,
    pub btc_usd_cents: i64,
    pub fetched_at: i64,
}

pub struct PriceCache {
    client: reqwest::Client,
    cached: Mutex<Option<Prices>>,
}

impl PriceCache {
    pub fn new() -> Result<Self, PriceError> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?,
            cached: Mutex::new(None),
        })
    }

    pub async fn prices(&self, now: i64) -> Result<Prices, PriceError> {
        let mut cached = self.cached.lock().await;
        if let Some(p) = *cached
            && now >= p.fetched_at
            && now - p.fetched_at < CACHE_SECS
        {
            return Ok(p);
        }
        let body = self
            .client
            .get(TOKENS_URL)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let prices = Prices {
            zec_usd_cents: parse_usd_cents(&body, ZEC_ASSET)?,
            btc_usd_cents: parse_usd_cents(&body, BTC_ASSET)?,
            fetched_at: now,
        };
        *cached = Some(prices);
        Ok(prices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_live_fixture_price_as_cents() {
        let fixture = include_str!("../fixtures/price/tokens.json");
        assert_eq!(parse_usd_cents(fixture, ZEC_ASSET).unwrap(), 128_777);
        assert_eq!(parse_usd_cents(fixture, BTC_ASSET).unwrap(), 7_834_400);
    }

    #[test]
    fn rounds_decimal_prices_without_floats() {
        assert_eq!(decimal_cents("1.234").unwrap(), 123);
        assert_eq!(decimal_cents("1.235").unwrap(), 124);
        assert!(decimal_cents("1.2e2").is_err());
        assert!(decimal_cents("-1").is_err());
    }
}
