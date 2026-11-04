//! Quote maths (FR-7, design §3.1). Pure: no I/O, the clock is a parameter, integers only.

pub const MAX_PRICE_AGE_SECS: i64 = 120;
const ZAT_PER_ZEC: u128 = 100_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quote {
    pub price_zat: i64,
    pub fee_zat: i64,
    pub total_zat: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteError {
    StalePrice,
    NonPositiveInput,
    Overflow,
}

/// `ceil(usd_cents * 1e8 / zec_usd_cents)`; `zec_usd_cents` is USD cents per whole ZEC.
pub fn usd_cents_to_zat(usd_cents: i64, zec_usd_cents: i64) -> Result<i64, QuoteError> {
    if usd_cents <= 0 || zec_usd_cents <= 0 {
        return Err(QuoteError::NonPositiveInput);
    }
    // Both factors are positive i64, so the u128 product and the ceil-add cannot overflow.
    let divisor = zec_usd_cents as u128;
    let numerator = usd_cents as u128 * ZAT_PER_ZEC;
    let zat = numerator.div_ceil(divisor);
    i64::try_from(zat).map_err(|_| QuoteError::Overflow)
}

pub fn build(
    usd_cents: i64,
    zec_usd_cents: i64,
    fee_zat: i64,
    price_updated_at: i64,
    now: i64,
) -> Result<Quote, QuoteError> {
    if fee_zat < 0 {
        return Err(QuoteError::NonPositiveInput);
    }
    let age = now.saturating_sub(price_updated_at);
    if !(0..=MAX_PRICE_AGE_SECS).contains(&age) {
        // Includes a price stamped in the future (clock skew): never trust it.
        return Err(QuoteError::StalePrice);
    }
    let price_zat = usd_cents_to_zat(usd_cents, zec_usd_cents)?;
    let total_zat = price_zat.checked_add(fee_zat).ok_or(QuoteError::Overflow)?;
    Ok(Quote {
        price_zat,
        fee_zat,
        total_zat,
    })
}

/// Parses Bitrefill's decimal price string ("3.88", "10", "0.115") to whole cents,
/// rounding any sub-cent remainder UP so we never undercharge. Rejects empty, signed,
/// non-decimal and zero values with `NonPositiveInput`.
pub fn parse_usd_cents(s: &str) -> Result<i64, QuoteError> {
    const BAD: QuoteError = QuoteError::NonPositiveInput;
    let (whole, frac) = match s.split_once('.') {
        Some((_, "")) => return Err(BAD),
        Some(parts) => parts,
        None => (s, ""),
    };
    let all_digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || !all_digits(whole) || !all_digits(frac) {
        return Err(BAD);
    }
    // Digits are verified, so a parse failure here can only mean too large.
    let whole_cents = whole
        .parse::<i64>()
        .ok()
        .and_then(|w| w.checked_mul(100))
        .ok_or(QuoteError::Overflow)?;
    let mut digits = frac.bytes().map(|b| i64::from(b - b'0'));
    let tens = digits.next().unwrap_or(0);
    let ones = digits.next().unwrap_or(0);
    let sub_cent_remainder = digits.any(|d| d != 0);
    let cents = whole_cents
        .checked_add(tens * 10 + ones + i64::from(sub_cent_remainder))
        .ok_or(QuoteError::Overflow)?;
    if cents == 0 {
        return Err(BAD);
    }
    Ok(cents)
}

const SATS_PER_BTC: u128 = 100_000_000;

/// Bitrefill satoshi price → USD cents at `btc_usd_cents` per BTC, rounded UP (never undercharge).
pub fn sats_to_usd_cents(sats: i64, btc_usd_cents: i64) -> Result<i64, QuoteError> {
    if sats <= 0 || btc_usd_cents <= 0 {
        return Err(QuoteError::NonPositiveInput);
    }
    let cents = (sats as u128 * btc_usd_cents as u128).div_ceil(SATS_PER_BTC);
    i64::try_from(cents).map_err(|_| QuoteError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sats_to_cents_rounds_up_and_matches_live_jio_10() {
        // ₹10 Jio talktime was 141 sat with BTC at $78,344 → $0.1105 → 12 cents.
        assert_eq!(sats_to_usd_cents(141, 7_834_400), Ok(12));
        assert_eq!(sats_to_usd_cents(100_000_000, 7_834_400), Ok(7_834_400));
        assert_eq!(sats_to_usd_cents(0, 1), Err(QuoteError::NonPositiveInput));
        assert_eq!(sats_to_usd_cents(1, -1), Err(QuoteError::NonPositiveInput));
    }

    const NOW: i64 = 1_000_000;

    #[test]
    fn conversion_rounds_up_never_down() {
        // 1 cent at 3 cents/ZEC = 33_333_333.33.. zat -> 33_333_334
        assert_eq!(usd_cents_to_zat(1, 3), Ok(33_333_334));
    }

    #[test]
    fn conversion_exact_division_is_not_bumped() {
        assert_eq!(usd_cents_to_zat(1, 1), Ok(100_000_000));
        assert_eq!(usd_cents_to_zat(388, 4000), Ok(9_700_000));
    }

    #[test]
    fn conversion_rejects_zero_and_negative() {
        for (usd, zec) in [(0, 100), (-1, 100), (100, 0), (100, -5), (0, 0)] {
            assert_eq!(
                usd_cents_to_zat(usd, zec),
                Err(QuoteError::NonPositiveInput),
                "usd={usd} zec={zec}"
            );
        }
    }

    #[test]
    fn conversion_overflow_past_i64_max() {
        assert_eq!(usd_cents_to_zat(i64::MAX, 1), Err(QuoteError::Overflow));
        // Largest value that still fits: i64::MAX zat.
        assert_eq!(usd_cents_to_zat(i64::MAX, 100_000_000), Ok(i64::MAX));
    }

    #[test]
    fn build_sums_price_and_fee() {
        let q = build(388, 4000, 10_000, NOW - 5, NOW).unwrap();
        assert_eq!(
            q,
            Quote {
                price_zat: 9_700_000,
                fee_zat: 10_000,
                total_zat: 9_710_000
            }
        );
    }

    #[test]
    fn build_accepts_zero_fee() {
        let q = build(388, 4000, 0, NOW, NOW).unwrap();
        assert_eq!(q.total_zat, q.price_zat);
    }

    #[test]
    fn build_price_age_boundary_120_vs_121() {
        assert!(build(388, 4000, 0, NOW - 120, NOW).is_ok());
        assert_eq!(
            build(388, 4000, 0, NOW - 121, NOW),
            Err(QuoteError::StalePrice)
        );
    }

    #[test]
    fn build_future_timestamp_is_stale() {
        assert_eq!(
            build(388, 4000, 0, NOW + 1, NOW),
            Err(QuoteError::StalePrice)
        );
    }

    #[test]
    fn build_rejects_non_positive_inputs() {
        let bad = Err(QuoteError::NonPositiveInput);
        assert_eq!(build(0, 4000, 0, NOW, NOW), bad);
        assert_eq!(build(-1, 4000, 0, NOW, NOW), bad);
        assert_eq!(build(388, 0, 0, NOW, NOW), bad);
        assert_eq!(build(388, -1, 0, NOW, NOW), bad);
        assert_eq!(build(388, 4000, -1, NOW, NOW), bad);
    }

    #[test]
    fn build_overflow_on_price_and_on_total() {
        assert_eq!(build(i64::MAX, 1, 0, NOW, NOW), Err(QuoteError::Overflow));
        assert_eq!(
            build(i64::MAX, 100_000_000, 1, NOW, NOW),
            Err(QuoteError::Overflow)
        );
    }

    #[test]
    fn parse_whole_and_fractional_prices() {
        assert_eq!(parse_usd_cents("0.11"), Ok(11));
        assert_eq!(parse_usd_cents("3.88"), Ok(388));
        assert_eq!(parse_usd_cents("10"), Ok(1000));
        assert_eq!(parse_usd_cents("0.5"), Ok(50));
        assert_eq!(parse_usd_cents("1.10"), Ok(110));
        assert_eq!(parse_usd_cents("007.05"), Ok(705));
    }

    #[test]
    fn parse_rounds_sub_cent_remainder_up() {
        assert_eq!(parse_usd_cents("0.115"), Ok(12));
        assert_eq!(parse_usd_cents("0.111"), Ok(12));
        assert_eq!(parse_usd_cents("0.1100"), Ok(11));
        assert_eq!(parse_usd_cents("0.110001"), Ok(12));
        assert_eq!(parse_usd_cents("0.001"), Ok(1));
    }

    #[test]
    fn parse_rejects_empty_signed_garbage_and_zero() {
        for bad in [
            "", " ", "-1.00", "+1.00", "abc", "1.2.3", ".", "1.", ".5", "1e3", "1,00", " 1.00",
            "1.00 ", "0", "0.00", "1.0x", "١٢",
        ] {
            assert_eq!(
                parse_usd_cents(bad),
                Err(QuoteError::NonPositiveInput),
                "input {bad:?}"
            );
        }
    }

    #[test]
    fn parse_overflow_is_reported() {
        assert_eq!(
            parse_usd_cents("99999999999999999999"),
            Err(QuoteError::Overflow)
        );
        assert_eq!(
            parse_usd_cents("92233720368547758.08"),
            Err(QuoteError::Overflow)
        );
        assert_eq!(parse_usd_cents("92233720368547758.07"), Ok(i64::MAX));
    }
}
