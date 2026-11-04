use crate::model::Operator;
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

const REQUEST: &str = include_str!("../fixtures/intent/claude-intent-request.json");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    TopUp {
        operator: Operator,
        phone: String,
        inr: i64,
    },
    /// Operator and phone but no amount: show the recharge options as buttons.
    Choose {
        operator: Operator,
        phone: String,
    },
    Balance,
    Help,
    Unknown,
}

#[derive(Debug, Error)]
pub enum IntentError {
    #[error("invalid Claude response: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn parse_regex(text: &str) -> Intent {
    let lower = text.to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let operator = words.iter().find_map(|word| Operator::parse(word));
    let Some(operator) = operator else {
        if words.contains(&"balance") {
            return Intent::Balance;
        }
        if words.iter().any(|w| *w == "help" || *w == "start") {
            return Intent::Help;
        }
        return Intent::Unknown;
    };
    let groups: Vec<&str> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|g| !g.is_empty())
        .collect();
    let Some((phone, used)) = find_phone(&groups) else {
        return Intent::Unknown;
    };
    let amount = groups
        .iter()
        .enumerate()
        .filter(|(i, _)| !used.contains(i))
        .find_map(|(_, g)| g.parse::<i64>().ok().filter(|n| (1..=MAX_INR).contains(n)));
    match amount {
        Some(inr) => Intent::TopUp {
            operator,
            phone,
            inr,
        },
        None => Intent::Choose { operator, phone },
    }
}

/// Upper bound for a parsed amount; the real per-order limit is enforced in `caps`.
const MAX_INR: i64 = 100_000;
const MAX_PHONE_GROUPS: usize = 4;

/// First run of adjacent digit groups ("+91 98765 43210") that normalizes to a phone number.
/// Returns the phone and the indices of the groups it used.
fn find_phone(groups: &[&str]) -> Option<(String, std::ops::Range<usize>)> {
    (0..groups.len()).find_map(|i| {
        (i..groups.len().min(i + MAX_PHONE_GROUPS))
            .find_map(|j| normalize_phone(&groups[i..=j].concat()).map(|p| (p, i..j + 1)))
    })
}

/// Indian mobile: 10 digits starting 6-9. A `0091`, `91` or `0` prefix is stripped only when
/// there are more than 10 digits, so a number like 9191919191 stays intact.
fn normalize_phone(digits: &str) -> Option<String> {
    let mut d = digits;
    while d.len() > 10 {
        d = d
            .strip_prefix("0091")
            .or_else(|| d.strip_prefix("91"))
            .or_else(|| d.strip_prefix('0'))?;
    }
    (d.len() == 10
        && d.bytes().all(|b| b.is_ascii_digit())
        && matches!(d.as_bytes()[0], b'6'..=b'9'))
    .then(|| d.to_owned())
}

pub fn parse_claude_response(json: &str) -> Result<Intent, IntentError> {
    let json = json.rfind('}').map(|i| &json[..=i]).unwrap_or(json);
    let response: Value = serde_json::from_str(json)?;
    let stop = response.get("stop_reason").and_then(Value::as_str);
    if stop != Some("end_turn") {
        return Ok(Intent::Unknown);
    }
    let Some(content) = response
        .get("content")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .find(|v| v.get("type").and_then(Value::as_str) == Some("text"))
        })
        .and_then(|v| v.get("text"))
        .and_then(Value::as_str)
    else {
        return Ok(Intent::Unknown);
    };
    let result: Value = serde_json::from_str(content)?;
    if result.get("kind").and_then(Value::as_str) != Some("order") {
        return Ok(Intent::Unknown);
    }
    let Some(operator) = result
        .get("operator")
        .and_then(Value::as_str)
        .and_then(Operator::parse)
    else {
        return Ok(Intent::Unknown);
    };
    let Some(phone) = result
        .get("phone")
        .and_then(Value::as_str)
        .and_then(normalize_phone)
    else {
        return Ok(Intent::Unknown);
    };
    let Some(inr) = result
        .get("amount_inr")
        .and_then(Value::as_i64)
        .filter(|n| (1..=100_000).contains(n))
    else {
        return Ok(Intent::Unknown);
    };
    Ok(Intent::TopUp {
        operator,
        phone,
        inr,
    })
}

pub async fn extract(client: &reqwest::Client, api_key: &str, model: &str, text: &str) -> Intent {
    let fallback = || parse_regex(text);
    let Ok(mut request) = serde_json::from_str::<Value>(REQUEST) else {
        return fallback();
    };
    request["model"] = Value::String(model.to_owned());
    request["messages"][0]["content"] = Value::String(text.to_owned());
    request.as_object_mut().map(|o| o.remove("fallbacks"));
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await
    })
    .await;
    match result {
        Ok(Ok(body)) => match parse_claude_response(&body) {
            // The model answers Unknown for "jio 98…" with no amount; the parser turns that into Choose.
            Ok(intent) if intent != Intent::Unknown && phone_is_in_text(&intent, text) => intent,
            _ => fallback(),
        },
        _ => fallback(),
    }
}

/// The quote card masks the phone, so a model that invents or transposes digits would go
/// unnoticed. Accept its phone only if those 10 digits appear in what the user typed.
fn phone_is_in_text(intent: &Intent, text: &str) -> bool {
    match intent {
        Intent::TopUp { phone, .. } => {
            let digits: String = text.chars().filter(char::is_ascii_digit).collect();
            digits.contains(phone.as_str())
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regex_accepts_topups_and_indian_phone_prefixes() {
        for text in [
            "jio 9876543210 10",
            "airtel ₹20 to 98765 43210",
            "recharge vi 9876543210 for 50 rupees",
            "jio +91 9876543210 10",
            "airtel 0919876543210 10",
        ] {
            assert!(matches!(parse_regex(text), Intent::TopUp { .. }), "{text}");
        }
    }

    #[test]
    fn regex_extracts_exact_fields() {
        let top_up = |operator, inr| Intent::TopUp {
            operator,
            phone: "9876543210".into(),
            inr,
        };
        assert_eq!(parse_regex("jio 9876543210 10"), top_up(Operator::Jio, 10));
        assert_eq!(
            parse_regex("airtel ₹20 to 98765 43210"),
            top_up(Operator::Airtel, 20)
        );
        assert_eq!(
            parse_regex("recharge vi 9876543210 for 50 rupees"),
            top_up(Operator::Vi, 50)
        );
        assert_eq!(
            parse_regex("Jio +91 9876543210 10"),
            top_up(Operator::Jio, 10)
        );
    }

    #[test]
    fn regex_rejects_invalid_inputs() {
        for text in [
            "jio 5876543210 10",
            "jio 987654321 10",
            "jio 98765432101 10",
            "foo 9876543210 10",
        ] {
            assert_eq!(parse_regex(text), Intent::Unknown, "{text}");
        }
        assert_eq!(parse_regex("balance"), Intent::Balance);
        assert_eq!(parse_regex("help"), Intent::Help);
    }

    #[test]
    fn operator_and_phone_without_amount_asks_to_choose() {
        assert_eq!(
            parse_regex("jio 9876543210"),
            Intent::Choose {
                operator: Operator::Jio,
                phone: "9876543210".into()
            }
        );
    }

    #[test]
    fn parses_and_revalidates_claude_fixture() {
        let fixture = include_str!("../fixtures/intent/claude-intent-order.json");
        assert_eq!(
            parse_claude_response(fixture).unwrap(),
            Intent::TopUp {
                operator: Operator::Jio,
                phone: "9876543210".into(),
                inr: 199
            }
        );
        let tampered = fixture.replace("9876543210", "98765432101");
        assert_eq!(parse_claude_response(&tampered).unwrap(), Intent::Unknown);
    }
}
