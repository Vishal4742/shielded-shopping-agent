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

pub fn parse_zec_usd_cents(json: &str) -> Result<i64, PriceError> {
    let tokens: Vec<Token<'_>> = serde_json::from_str(json)?;
    let raw = tokens
        .iter()
        .find(|token| token.asset_id == "nep141:zec.omft.near")
        .ok_or(PriceError::InvalidPrice)?
        .price
        .get();
    decimal_cents(raw)
}

fn decimal_cents(raw: &str) -> Result<i64, PriceError> {
    if raw.starts_with('-') || raw.starts_with('+') {
        return Err(PriceError::InvalidPrice);
    }
    let (mantissa, exponent) = raw
        .split_once(['e', 'E'])
        .map(|(m, e)| (m, e.parse::<i32>().ok()))
        .unwrap_or((raw, Some(0)));
    let exponent = exponent.ok_or(PriceError::InvalidPrice)?;
    let (whole, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(PriceError::InvalidPrice);
    }
    let digits = format!("{whole}{frac}");
    let n = digits
        .parse::<i128>()
        .map_err(|_| PriceError::InvalidPrice)?;
    let scale = exponent - frac.len() as i32 + 2;
    let cents = if scale >= 0 {
        n.checked_mul(
            10_i128
                .checked_pow(scale as u32)
                .ok_or(PriceError::InvalidPrice)?,
        )
    } else {
        let divisor = 10_i128
            .checked_pow((-scale) as u32)
            .ok_or(PriceError::InvalidPrice)?;
        Some((n + divisor / 2) / divisor)
    }
    .ok_or(PriceError::InvalidPrice)?;
    i64::try_from(cents).map_err(|_| PriceError::InvalidPrice)
}

pub struct PriceCache {
    client: reqwest::Client,
    cached: Mutex<Option<(i64, i64)>>,
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

    pub async fn zec_usd_cents(&self, now: i64) -> Result<(i64, i64), PriceError> {
        let mut cached = self.cached.lock().await;
        if let Some((cents, fetched_at)) = *cached {
            if now >= fetched_at && now - fetched_at < 60 {
                return Ok((cents, fetched_at));
            }
        }
        let body = self
            .client
            .get(TOKENS_URL)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let value = parse_zec_usd_cents(&body)?;
        *cached = Some((value, now));
        Ok((value, now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_live_fixture_price_as_cents() {
        let fixture = include_str!("../fixtures/price/tokens.json");
        assert_eq!(parse_zec_usd_cents(fixture).unwrap(), 128_777);
    }

    #[test]
    fn rounds_decimal_prices_without_floats() {
        assert_eq!(decimal_cents("1.234").unwrap(), 123);
        assert_eq!(decimal_cents("1.235").unwrap(), 124);
        assert_eq!(decimal_cents("1.2e2").unwrap(), 12_000);
    }
}
