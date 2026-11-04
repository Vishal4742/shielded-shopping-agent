//! ZIP-321 payment URI builder (FR-3). Pure. Build only: no parser, no percent-encoding.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Zip321Error {
    #[error("address is empty")]
    EmptyAddress,
    #[error("address must not carry a URI scheme")]
    UnsupportedScheme,
    #[error("amount must not be negative")]
    NegativeAmount,
    #[error("memo exceeds 512 bytes")]
    MemoTooLong,
}

const ZAT_PER_ZEC: i64 = 100_000_000;
/// Zcash shielded memo field size.
const MAX_MEMO_BYTES: usize = 512;
const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// `zcash:<address>[?amount=<ZEC>][&memo=<base64url>]`.
///
/// ponytail: the address is trusted (our own wallet UA) and not percent-encoded; add
/// validation if a caller ever passes user-supplied addresses.
pub fn deposit_uri(
    address: &str,
    amount_zat: Option<i64>,
    memo: Option<&str>,
) -> Result<String, Zip321Error> {
    if address.is_empty() {
        return Err(Zip321Error::EmptyAddress);
    }
    if address.contains(':') {
        return Err(Zip321Error::UnsupportedScheme);
    }
    let mut params = Vec::new();
    if let Some(zat) = amount_zat {
        if zat < 0 {
            return Err(Zip321Error::NegativeAmount);
        }
        params.push(format!("amount={}", format_zec(zat)));
    }
    if let Some(memo) = memo.filter(|m| !m.is_empty()) {
        if memo.len() > MAX_MEMO_BYTES {
            return Err(Zip321Error::MemoTooLong);
        }
        params.push(format!("memo={}", base64url(memo.as_bytes())));
    }
    if params.is_empty() {
        Ok(format!("zcash:{address}"))
    } else {
        Ok(format!("zcash:{address}?{}", params.join("&")))
    }
}

/// Exact decimal ZEC, trailing zeros trimmed, no trailing point (`150000000` -> `1.5`).
fn format_zec(zat: i64) -> String {
    let whole = zat / ZAT_PER_ZEC;
    let frac = format!("{:08}", zat % ZAT_PER_ZEC);
    match frac.trim_end_matches('0') {
        "" => whole.to_string(),
        digits => format!("{whole}.{digits}"),
    }
}

/// RFC 4648 §5 base64url without padding.
fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, &b)| acc | (u32::from(b) << (16 - 8 * i)));
        // A chunk of k bytes yields k + 1 sextets.
        for i in 0..=chunk.len() {
            out.push(char::from(BASE64URL[((n >> (18 - 6 * i)) & 63) as usize]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDR: &str = "utest1abcdefghijklmnop";

    #[test]
    fn no_amount_gives_bare_uri() {
        assert_eq!(
            deposit_uri(ADDR, None, None).unwrap(),
            format!("zcash:{ADDR}")
        );
    }

    #[test]
    fn one_zec() {
        assert_eq!(
            deposit_uri(ADDR, Some(100_000_000), None).unwrap(),
            format!("zcash:{ADDR}?amount=1")
        );
    }

    #[test]
    fn one_zat() {
        assert_eq!(
            deposit_uri(ADDR, Some(1), None).unwrap(),
            format!("zcash:{ADDR}?amount=0.00000001")
        );
    }

    #[test]
    fn mixed_amount_trims_trailing_zeros() {
        assert_eq!(
            deposit_uri(ADDR, Some(12_345_678), None).unwrap(),
            format!("zcash:{ADDR}?amount=0.12345678")
        );
        assert_eq!(
            deposit_uri(ADDR, Some(150_000_000), None).unwrap(),
            format!("zcash:{ADDR}?amount=1.5")
        );
    }

    #[test]
    fn zero_amount_is_zero() {
        assert_eq!(
            deposit_uri(ADDR, Some(0), None).unwrap(),
            format!("zcash:{ADDR}?amount=0")
        );
    }

    #[test]
    fn i64_max_does_not_overflow() {
        assert_eq!(
            deposit_uri(ADDR, Some(i64::MAX), None).unwrap(),
            format!("zcash:{ADDR}?amount=92233720368.54775807")
        );
    }

    #[test]
    fn negative_amount_rejected() {
        assert_eq!(
            deposit_uri(ADDR, Some(-1), None),
            Err(Zip321Error::NegativeAmount)
        );
    }

    #[test]
    fn empty_address_rejected() {
        assert_eq!(deposit_uri("", None, None), Err(Zip321Error::EmptyAddress));
    }

    #[test]
    fn scheme_prefix_rejected() {
        assert_eq!(
            deposit_uri("zcash:utest1abc", None, None),
            Err(Zip321Error::UnsupportedScheme)
        );
    }

    #[test]
    fn memo_known_vector() {
        assert_eq!(
            deposit_uri(ADDR, None, Some("Thank you for your purchase")).unwrap(),
            format!("zcash:{ADDR}?memo=VGhhbmsgeW91IGZvciB5b3VyIHB1cmNoYXNl")
        );
    }

    #[test]
    fn memo_and_amount_join_with_ampersand() {
        assert_eq!(
            deposit_uri(ADDR, Some(100_000_000), Some("Thank you for your purchase")).unwrap(),
            format!("zcash:{ADDR}?amount=1&memo=VGhhbmsgeW91IGZvciB5b3VyIHB1cmNoYXNl")
        );
    }

    #[test]
    fn memo_padding_cases_have_no_equals() {
        // RFC 4648 vectors: every padding length, with '=' stripped.
        for (memo, b64) in [
            ("f", "Zg"),
            ("fo", "Zm8"),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg"),
        ] {
            assert_eq!(
                deposit_uri(ADDR, None, Some(memo)).unwrap(),
                format!("zcash:{ADDR}?memo={b64}")
            );
        }
    }

    #[test]
    fn memo_uses_url_safe_alphabet_and_utf8() {
        // "₹" = E2 82 B9 -> "4oK5"; "~~~" = 7E 7E 7E -> "fn5-" (std base64 gives "fn5+");
        // "???" = 3F 3F 3F -> "Pz8_" (std gives "Pz8/").
        for (memo, b64) in [("₹", "4oK5"), ("~~~", "fn5-"), ("???", "Pz8_")] {
            assert_eq!(
                deposit_uri(ADDR, None, Some(memo)).unwrap(),
                format!("zcash:{ADDR}?memo={b64}")
            );
        }
    }

    #[test]
    fn memo_limit_is_512_bytes() {
        let ok = "a".repeat(512);
        assert!(deposit_uri(ADDR, None, Some(&ok)).is_ok());
        let too_long = "a".repeat(513);
        assert_eq!(
            deposit_uri(ADDR, None, Some(&too_long)),
            Err(Zip321Error::MemoTooLong)
        );
        // 171 * 3 bytes = 513 bytes: the limit is bytes, not chars.
        assert_eq!(
            deposit_uri(ADDR, None, Some(&"₹".repeat(171))),
            Err(Zip321Error::MemoTooLong)
        );
    }

    #[test]
    fn empty_memo_is_omitted() {
        assert_eq!(
            deposit_uri(ADDR, None, Some("")).unwrap(),
            format!("zcash:{ADDR}")
        );
    }
}
