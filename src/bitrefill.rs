//! Bitrefill v2 client (EI-3, FR-9, FR-14). The only module that talks to Bitrefill.
//!
//! Pure parsers (`parse_product`, `parse_invoice_status`, `invoice_body`) are split from the HTTP
//! calls so they test on fixtures. Nothing here logs the API key, a full phone number or any
//! redemption data.

use std::fmt;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

pub const DEFAULT_BASE: &str = "https://api.bitrefill.com/v2";
const TIMEOUT: Duration = Duration::from_secs(20);
const BODY_EXCERPT_CHARS: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Non-2xx answer (or a transport failure, status 0). `body` is a short excerpt with the API
    /// key scrubbed.
    #[error("bitrefill http {status}: {body}")]
    Http { status: u16, body: String },
    #[error("bitrefill timeout")]
    Timeout,
    #[error("bitrefill rate limited")]
    RateLimited,
    #[error("bitrefill decode: {0}")]
    Decode(String),
    /// `BITREFILL_DRY_RUN=1`: the purchase was skipped before any network call.
    #[error("dry run: invoice not created")]
    DryRun,
    #[error("bitrefill client setup: {0}")]
    Client(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Exact `value` string from the catalog (case-sensitive, en dashes). Copy, never retype.
    pub value: String,
    /// Face value in rupees (`amount` in the catalog). Absent on non-INR or ranged items.
    pub inr: Option<i64>,
    /// Catalog `price`: integer minor units of the **account** currency. The account is BTC today,
    /// so this is satoshis, not USD cents (notes/bitrefill.md). It moves with the BTC rate.
    // ponytail: no usd_cents until the account is switched to USD; then confirm the unit on a real fixture.
    pub price: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    pub slug: String,
    pub name: String,
    pub packages: Vec<Package>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvoiceStatus {
    Pending,
    /// Money taken, top-up not confirmed yet. NOT delivered.
    Paid,
    Delivered,
    Failed,
    Unknown(String),
}

pub struct Bitrefill {
    http: reqwest::Client,
    base: String,
    api_key: String,
    dry_run: bool,
}

impl fmt::Debug for Bitrefill {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bitrefill")
            .field("base", &self.base)
            .field("api_key", &"<redacted>")
            .field("dry_run", &self.dry_run)
            .finish()
    }
}

impl Bitrefill {
    pub fn new(base: impl Into<String>, api_key: impl Into<String>, dry_run: bool) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|e| Error::Client(e.to_string()))?;
        Ok(Self {
            http,
            base: base.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            dry_run,
        })
    }

    pub async fn get_product(&self, slug: &str) -> Result<Product> {
        let body = self
            .send(self.http.get(format!("{}/products/{slug}", self.base)))
            .await?;
        parse_product(&body)
    }

    /// Buys one package with the account balance and pays immediately (`auto_pay`).
    /// The API has no reference field, so `order_id` is only logged; the caller stores the
    /// returned invoice id against the order (design §3.3).
    pub async fn create_invoice(
        &self,
        product_id: &str,
        package_value: &str,
        phone: &str,
        order_id: &str,
    ) -> Result<InvoiceId> {
        if self.dry_run {
            return Err(Error::DryRun);
        }
        tracing::info!(order_id, product_id, phone = %mask_phone(phone), "bitrefill create_invoice");
        let req = self
            .http
            .post(format!("{}/invoices", self.base))
            .json(&invoice_body(product_id, package_value, phone));
        let body = self.send(req).await?;
        parse_invoice_id(&body)
    }

    pub async fn get_invoice(&self, id: &InvoiceId) -> Result<InvoiceStatus> {
        let body = self
            .send(self.http.get(format!("{}/invoices/{}", self.base, id.0)))
            .await?;
        parse_invoice_status(&body)
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<String> {
        let resp = req
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| self.map_transport(&e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| self.map_transport(&e))?;
        if status.as_u16() == 429 {
            return Err(Error::RateLimited);
        }
        if !status.is_success() {
            return Err(Error::Http {
                status: status.as_u16(),
                body: self.excerpt(&text),
            });
        }
        Ok(text)
    }

    fn map_transport(&self, e: &reqwest::Error) -> Error {
        if e.is_timeout() {
            return Error::Timeout;
        }
        // The key travels in a header, never the URL; `excerpt` scrubs it regardless.
        let text = e.to_string();
        Error::Http {
            status: 0,
            body: self.excerpt(&text),
        }
    }

    fn excerpt(&self, text: &str) -> String {
        let scrubbed = if self.api_key.is_empty() {
            text.to_string()
        } else {
            text.replace(&self.api_key, "<redacted>")
        };
        scrubbed
            .chars()
            .take(BODY_EXCERPT_CHARS)
            .collect::<String>()
            .replace('\n', " ")
    }
}

/// Keeps only the last 2 characters: `+919876543210` -> `***********10`.
pub fn mask_phone(phone: &str) -> String {
    let n = phone.chars().count();
    phone
        .chars()
        .enumerate()
        .map(|(i, c)| if i + 2 >= n { c } else { '*' })
        .collect()
}

/// `package_id` is the product slug + `<&>` + the exact package value, as the catalog lists it.
/// Bitrefill wants E.164. Our orders store 10-digit Indian mobiles; a bare number fails with
/// HTTP 500 "Number does not match operator's country" (seen live, 4 Nov).
pub fn e164_india(phone: &str) -> String {
    if phone.starts_with('+') {
        phone.to_owned()
    } else {
        format!("+91{phone}")
    }
}

pub fn invoice_body(product_id: &str, package_value: &str, phone: &str) -> Value {
    json!({
        "products": [{
            "product_id": product_id,
            "package_id": format!("{product_id}<&>{package_value}"),
            "phone_number": e164_india(phone),
            "quantity": 1,
        }],
        "payment_method": "balance",
        "auto_pay": true,
    })
}

#[derive(Deserialize)]
struct RawPackage {
    value: String,
    amount: Option<i64>,
    price: i64,
}

#[derive(Deserialize)]
struct RawProduct {
    id: String,
    name: String,
    #[serde(default)]
    packages: Vec<RawPackage>,
}

/// The API wraps payloads as `{"meta":…,"data":…}`; accept a bare object too.
fn payload(v: &Value) -> &Value {
    v.get("data").unwrap_or(v)
}

fn decode_err(e: impl fmt::Display) -> Error {
    Error::Decode(e.to_string())
}

pub fn parse_product(json: &str) -> Result<Product> {
    let v: Value = serde_json::from_str(json).map_err(decode_err)?;
    let raw = RawProduct::deserialize(payload(&v)).map_err(decode_err)?;
    Ok(Product {
        slug: raw.id,
        name: raw.name,
        packages: raw
            .packages
            .into_iter()
            .map(|p| Package {
                value: p.value,
                inr: p.amount,
                price: p.price,
            })
            .collect(),
    })
}

fn parse_invoice_id(json: &str) -> Result<InvoiceId> {
    let v: Value = serde_json::from_str(json).map_err(decode_err)?;
    match payload(&v).get("id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() => Ok(InvoiceId(id.to_string())),
        _ => Err(Error::Decode("invoice response has no id".into())),
    }
}

/// Invoice states: unpaid, payment_detected, payment_confirmed, pending, complete, blocked,
/// denied, payment_error. Order states: created, payment_detected, processing, delivered, failed,
/// refunded. Delivery is an *order* fact, so only all-orders-delivered counts as Delivered.
pub fn parse_invoice_status(json: &str) -> Result<InvoiceStatus> {
    let v: Value = serde_json::from_str(json).map_err(decode_err)?;
    let inv = payload(&v);
    let status = inv
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Decode("invoice has no status".into()))?;
    let orders: Vec<&str> = inv
        .get("orders")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|o| o.get("status").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();

    if orders.iter().any(|s| matches!(*s, "failed" | "refunded")) {
        return Ok(InvoiceStatus::Failed);
    }
    if !orders.is_empty() && orders.iter().all(|s| *s == "delivered") {
        return Ok(InvoiceStatus::Delivered);
    }
    Ok(match status {
        "denied" | "payment_error" => InvoiceStatus::Failed,
        "complete" | "payment_confirmed" => InvoiceStatus::Paid,
        "unpaid" | "payment_detected" | "pending" => InvoiceStatus::Pending,
        // blocked, not_delivered and anything new: let the caller treat it as UNCERTAIN.
        other => InvoiceStatus::Unknown(other.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const JIO: &str = include_str!("../fixtures/bitrefill/reliance-jio-india-bundles.json");
    const AIRTEL: &str = include_str!("../fixtures/bitrefill/airtel-india.json");
    const VI: &str = include_str!("../fixtures/bitrefill/vi-india.json");
    // ponytail: synthetic until first real invoice (shape from docs.bitrefill.com Invoice schema).
    const INVOICE_DELIVERED: &str =
        include_str!("../fixtures/bitrefill/invoice-delivered.synthetic.json");

    fn invoice(status: &str, orders: &[&str]) -> String {
        let os: Vec<Value> = orders
            .iter()
            .map(|s| json!({"id": "o", "status": s}))
            .collect();
        json!({"data": {"id": "inv1", "status": status, "orders": os}}).to_string()
    }

    #[test]
    fn airtel_fixture_parses_packages() {
        let p = parse_product(AIRTEL).unwrap();
        assert_eq!(p.slug, "airtel-india");
        assert_eq!(p.name, "Airtel India");
        assert_eq!(
            p.packages,
            vec![
                Package {
                    value: "500".into(),
                    inr: Some(500),
                    price: 7006
                },
                Package {
                    value: "1000".into(),
                    inr: Some(1000),
                    price: 14011
                },
            ]
        );
    }

    #[test]
    fn vi_fixture_keeps_odd_value_string() {
        let p = parse_product(VI).unwrap();
        assert_eq!(p.packages[0].value, "get talktime of rs. 423.73");
        assert_eq!(p.packages[0].inr, Some(500));
    }

    #[test]
    fn jio_fixture_keeps_en_dash_and_finds_cheapest() {
        let p = parse_product(JIO).unwrap();
        assert!(
            p.packages
                .iter()
                .any(|x| x.value == "30GB – 30 Day Data Pack"),
            "en dash must survive"
        );
        let cheapest = p.packages.iter().min_by_key(|x| x.price).unwrap();
        assert_eq!(cheapest.value, "₹7.47 Talktime Top-Up");
        assert_eq!(cheapest.inr, Some(10));
    }

    #[test]
    fn product_without_packages_is_empty_not_error() {
        let p = parse_product(r#"{"data":{"id":"x","name":"X"}}"#).unwrap();
        assert!(p.packages.is_empty());
    }

    #[test]
    fn bad_product_json_is_decode_error() {
        assert!(matches!(parse_product("nope"), Err(Error::Decode(_))));
        assert!(matches!(
            parse_product(r#"{"data":{"id":"x"}}"#),
            Err(Error::Decode(_))
        ));
    }

    #[test]
    fn synthetic_delivered_invoice_is_delivered() {
        assert_eq!(
            parse_invoice_status(INVOICE_DELIVERED).unwrap(),
            InvoiceStatus::Delivered
        );
        assert_eq!(
            parse_invoice_id(INVOICE_DELIVERED).unwrap(),
            InvoiceId("c2b27180-610e-4132-af77-ad42fc0ac444".into())
        );
    }

    #[test]
    fn paid_is_not_delivered() {
        let paid = |s, o: &[&str]| parse_invoice_status(&invoice(s, o)).unwrap();
        assert_eq!(paid("complete", &["processing"]), InvoiceStatus::Paid);
        assert_eq!(paid("payment_confirmed", &[]), InvoiceStatus::Paid);
        // complete with no order detail must not claim delivery
        assert_eq!(paid("complete", &[]), InvoiceStatus::Paid);
    }

    #[test]
    fn pending_failed_and_unknown_states() {
        let st = |s, o: &[&str]| parse_invoice_status(&invoice(s, o)).unwrap();
        for s in ["unpaid", "payment_detected", "pending"] {
            assert_eq!(st(s, &["created"]), InvoiceStatus::Pending);
        }
        assert_eq!(st("denied", &[]), InvoiceStatus::Failed);
        assert_eq!(st("payment_error", &[]), InvoiceStatus::Failed);
        assert_eq!(st("complete", &["failed"]), InvoiceStatus::Failed);
        assert_eq!(st("complete", &["refunded"]), InvoiceStatus::Failed);
        assert_eq!(st("blocked", &[]), InvoiceStatus::Unknown("blocked".into()));
        assert!(matches!(
            parse_invoice_status(r#"{"data":{}}"#),
            Err(Error::Decode(_))
        ));
    }

    #[test]
    fn invoice_phone_is_e164_for_bare_indian_numbers() {
        let b = invoice_body("reliance-jio-india-bundles", "10", "6299200082");
        assert_eq!(b["products"][0]["phone_number"], "+916299200082");
        let b = invoice_body("reliance-jio-india-bundles", "10", "+916299200082");
        assert_eq!(b["products"][0]["phone_number"], "+916299200082");
    }

    #[test]
    fn invoice_body_uses_balance_autopay_and_catalog_package_id() {
        let b = invoice_body("airtel-india", "500", "+919876543210");
        assert_eq!(b["payment_method"], "balance");
        assert_eq!(b["auto_pay"], true);
        let p = &b["products"][0];
        assert_eq!(p["package_id"], "airtel-india<&>500");
        assert_eq!(p["phone_number"], "+919876543210");
        assert_eq!(p["quantity"], 1);
    }

    #[test]
    fn phone_mask_keeps_last_two_digits() {
        assert_eq!(mask_phone("+919876543210"), "***********10");
        assert_eq!(mask_phone("1"), "1");
        assert_eq!(mask_phone(""), "");
    }

    #[test]
    fn debug_redacts_api_key() {
        let c = Bitrefill::new(DEFAULT_BASE, "sk_super_secret", false).unwrap();
        let s = format!("{c:?}");
        assert!(!s.contains("sk_super_secret"));
        assert!(s.contains("redacted"));
    }

    #[test]
    fn excerpt_scrubs_key_and_truncates() {
        let c = Bitrefill::new(DEFAULT_BASE, "sk_super_secret", false).unwrap();
        let e = c.excerpt(&format!("bad key sk_super_secret {}", "x".repeat(500)));
        assert!(!e.contains("sk_super_secret"));
        assert!(e.chars().count() <= BODY_EXCERPT_CHARS + "<redacted>".len());
    }

    #[tokio::test]
    async fn dry_run_creates_no_invoice_and_makes_no_network_call() {
        // Port 1 refuses connections: any network attempt would be Http/Timeout, not DryRun.
        let c = Bitrefill::new("http://127.0.0.1:1", "k", true).unwrap();
        let r = c
            .create_invoice("airtel-india", "500", "+919876543210", "ord1")
            .await;
        assert!(matches!(r, Err(Error::DryRun)));
    }
}
