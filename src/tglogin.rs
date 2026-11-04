//! Telegram Login Widget check (core.telegram.org/widgets/login#checking-authorization).
//! The website sends back the widget's fields; we trust the Telegram id only if the hash is
//! HMAC-SHA256 of the sorted `key=value` lines, keyed with SHA-256 of the bot token.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

/// A login older than this is refused, so a leaked login stops working within the hour.
pub const MAX_AGE_SECS: i64 = 3_600;

/// Returns the Telegram user id when `query` (the widget's fields, URL-encoded) is genuine
/// and fresh at `now`.
pub fn verify(query: &str, bot_token: &str, now: i64) -> Option<i64> {
    let mut fields: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let hash_at = fields.iter().position(|(k, _)| k == "hash")?;
    let given = hex::decode(fields.remove(hash_at).1).ok()?;
    fields.sort();
    let check = fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut mac = Hmac::<Sha256>::new_from_slice(&Sha256::digest(bot_token)).ok()?;
    mac.update(check.as_bytes());
    mac.verify_slice(&given).ok()?; // constant-time compare
    let field = |name: &str| {
        fields
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    let auth_date: i64 = field("auth_date")?.parse().ok()?;
    if now - auth_date > MAX_AGE_SECS || auth_date > now + 60 {
        return None;
    }
    field("id")?.parse().ok()
}

#[cfg(test)]
pub fn sign(fields: &[(&str, &str)], bot_token: &str) -> String {
    let mut sorted = fields.to_vec();
    sorted.sort();
    let check = sorted
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut mac = Hmac::<Sha256>::new_from_slice(&Sha256::digest(bot_token)).unwrap();
    mac.update(check.as_bytes());
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in fields {
        q.append_pair(k, v);
    }
    q.append_pair("hash", &hex::encode(mac.finalize().into_bytes()));
    q.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "123:abc";
    const NOW: i64 = 1_800_000_000;

    fn login(auth_date: i64) -> String {
        let d = auth_date.to_string();
        sign(
            &[("id", "42"), ("first_name", "Asha K"), ("auth_date", &d)],
            TOKEN,
        )
    }

    #[test]
    fn accepts_a_fresh_genuine_login() {
        assert_eq!(verify(&login(NOW - 10), TOKEN, NOW), Some(42));
    }

    #[test]
    fn rejects_a_tampered_id() {
        let q = login(NOW).replacen("id=42", "id=43", 1);
        assert_eq!(verify(&q, TOKEN, NOW), None);
    }

    #[test]
    fn rejects_another_bots_signature() {
        assert_eq!(verify(&login(NOW), "999:other", NOW), None);
    }

    #[test]
    fn rejects_stale_future_and_unsigned_logins() {
        assert_eq!(verify(&login(NOW - MAX_AGE_SECS - 1), TOKEN, NOW), None);
        assert_eq!(verify(&login(NOW + 3600), TOKEN, NOW), None);
        assert_eq!(verify("id=42&auth_date=1", TOKEN, NOW), None);
        assert_eq!(verify("id=42&hash=zz", TOKEN, NOW), None);
    }
}
