//! Order service (FR-7, FR-8, FR-9, FR-14): quote, ✅ confirm + purchase, cancel, recovery.
//! The only path to a purchase is `confirm_and_buy`, called from a ✅ button tap.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use uuid::Uuid;

use crate::bitrefill::{self, Bitrefill, InvoiceId, InvoiceStatus, Package, Product};
use crate::caps;
use crate::config::{self, Config};
use crate::db::{ConfirmOutcome, Db, NewOrder, Order};
use crate::model::Operator;
use crate::order_fsm::{self, Event};
use crate::price::PriceCache;
use crate::quote;
use crate::zec::Zec;

/// Everything the handlers and workers share.
pub struct App {
    pub cfg: Config,
    pub env_path: PathBuf,
    pub db: Arc<Db>,
    pub zec: Arc<Zec>,
    pub bitrefill: Bitrefill,
    pub prices: PriceCache,
}

// One short-lived value per request; boxing `Order` would only add noise.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum QuoteOutcome {
    Quoted(Order),
    /// Limits refuse it before we even quote.
    Refused(caps::Decision),
    /// Bitrefill has no package with that rupee value for the operator.
    NoPackage {
        available_inr: Vec<i64>,
    },
    NotRegistered,
    /// Price or catalog lookup failed; message is safe to show.
    Unavailable(String),
}

#[derive(Debug)]
pub enum BuyOutcome {
    Delivered(Order),
    /// Bitrefill failed before payment; a REVERSAL refunded the budget.
    Failed(Order),
    /// Outcome unknown; operator resolves it. Never auto-retried.
    Uncertain(Order),
    /// `BITREFILL_DRY_RUN=1`: debit + reversal happened, no invoice.
    DryRun(Order),
    Refused(caps::Decision),
    Paused,
    NotOwner,
    NotQuoted,
    Expired,
    InsufficientFunds {
        available_zat: i64,
        needed_zat: i64,
    },
}

/// ZIP-317 fee for one receipt send, measured on testnet (notes/devtool.md).
pub const FEE_ZAT: i64 = 25_000;
pub const QUOTE_TTL_SECS: i64 = 600;
const POLL_EVERY: Duration = Duration::from_secs(1);
const POLL_FOR: Duration = Duration::from_secs(15);

/// Serialises "check caps, then confirm" so two concurrent ✅ taps can't both pass the limits.
// ponytail: global lock across users; per-user + global accounting in SQL if load ever matters
static BUY_LOCK: Mutex<()> = Mutex::const_new(());

pub fn slug_for(op: Operator) -> &'static str {
    match op {
        Operator::Jio => "reliance-jio-india-bundles",
        Operator::Airtel => "airtel-india",
        Operator::Vi => "vi-india",
    }
}

/// Cheapest package with exactly this rupee face value.
pub fn pick_package(product: &Product, inr: i64) -> Option<&Package> {
    product
        .packages
        .iter()
        .filter(|p| p.inr == Some(inr))
        .min_by_key(|p| p.price)
}

fn available_inr(product: &Product) -> Vec<i64> {
    let mut v: Vec<i64> = product.packages.iter().filter_map(|p| p.inr).collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Apply `event` through `order_fsm`, then a conditional UPDATE. `false` if someone else moved it.
fn advance(db: &Db, order: &Order, event: Event, now: i64) -> anyhow::Result<bool> {
    let from = order_fsm::parse(&order.status)?;
    let to = order_fsm::apply(from, event)?;
    db.transition(
        &order.id,
        order_fsm::as_str(from),
        order_fsm::as_str(to),
        now,
    )
}

fn reload(db: &Db, id: &str) -> anyhow::Result<Order> {
    db.get_order(id)?
        .ok_or_else(|| anyhow::anyhow!("order {id} vanished"))
}

fn caps_for(
    db: &Db,
    tg_id: i64,
    operator: Operator,
    inr: i64,
    now: i64,
) -> anyhow::Result<caps::Decision> {
    let (start, end) = ist_day(now);
    Ok(caps::check(
        operator,
        inr,
        db.spent_today_inr(Some(tg_id), start, end)?,
        db.spent_today_inr(None, start, end)?,
        db.last_purchase_at(tg_id)?,
        now,
    ))
}

pub async fn create_quote(
    app: &App,
    tg_id: i64,
    operator: Operator,
    phone: &str,
    inr: i64,
    now: i64,
) -> anyhow::Result<QuoteOutcome> {
    let registered = app
        .db
        .get_user(tg_id)?
        .is_some_and(|u| u.account_uuid.is_some());
    if !registered {
        return Ok(QuoteOutcome::NotRegistered);
    }
    let decision = caps_for(&app.db, tg_id, operator, inr, now)?;
    if decision != caps::Decision::Allow {
        return Ok(QuoteOutcome::Refused(decision));
    }
    let slug = slug_for(operator);
    let product = match app.bitrefill.get_product(slug).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, slug, "catalog lookup failed");
            return Ok(QuoteOutcome::Unavailable(
                "Bitrefill catalog unavailable, try again".into(),
            ));
        }
    };
    let Some(package) = pick_package(&product, inr) else {
        return Ok(QuoteOutcome::NoPackage {
            available_inr: available_inr(&product),
        });
    };
    let prices = match app.prices.prices(now).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "price lookup failed");
            return Ok(QuoteOutcome::Unavailable(
                "ZEC price unavailable, try again".into(),
            ));
        }
    };
    let usd_cents = quote::sats_to_usd_cents(package.price, prices.btc_usd_cents)
        .map_err(|e| anyhow::anyhow!("sats to cents: {e:?}"))?;
    let q = quote::build(
        usd_cents,
        prices.zec_usd_cents,
        FEE_ZAT,
        prices.fetched_at,
        now,
    )
    .map_err(|e| anyhow::anyhow!("quote: {e:?}"))?;
    let new = NewOrder {
        id: Uuid::new_v4().to_string(),
        tg_id,
        operator: operator.as_str().into(),
        product_slug: slug.into(),
        phone: phone.into(),
        inr,
        usd_cents,
        price_zat: q.price_zat,
        fee_zat: q.fee_zat,
        dry_run: app.cfg.bitrefill_dry_run,
        quote_expires_at: now + QUOTE_TTL_SECS,
        created_at: now,
    };
    app.db.insert_quote(&new)?;
    Ok(QuoteOutcome::Quoted(reload(&app.db, &new.id)?))
}

/// ✅ tap: pause switch, caps, then the one confirm transaction (DEBIT), then Bitrefill.
pub async fn confirm_and_buy(
    app: &App,
    order_id: &str,
    tg_id: i64,
    now: i64,
) -> anyhow::Result<BuyOutcome> {
    if config::paused(&app.env_path) {
        return Ok(BuyOutcome::Paused);
    }
    let Some(order) = app.db.get_order(order_id)? else {
        return Ok(BuyOutcome::NotOwner);
    };
    if order.tg_id != tg_id {
        return Ok(BuyOutcome::NotOwner);
    }
    let operator = Operator::parse(&order.operator)
        .ok_or_else(|| anyhow::anyhow!("order has unknown operator"))?;
    {
        let _guard = BUY_LOCK.lock().await;
        let inr = order.inr;
        let check = |s: crate::db::Spend| {
            caps::check(
                operator,
                inr,
                s.user_inr,
                s.global_inr,
                s.last_purchase_at,
                now,
            )
        };
        match app.db.confirm_order_checked(
            order_id,
            tg_id,
            order.total_zat(),
            now,
            ist_day(now),
            check,
        )? {
            ConfirmOutcome::Refused(decision) => return Ok(BuyOutcome::Refused(decision)),
            ConfirmOutcome::Confirmed => {}
            ConfirmOutcome::NotOwner => return Ok(BuyOutcome::NotOwner),
            ConfirmOutcome::NotQuoted => return Ok(BuyOutcome::NotQuoted),
            ConfirmOutcome::Expired => {
                advance(&app.db, &order, Event::Expire, now)?;
                return Ok(BuyOutcome::Expired);
            }
            ConfirmOutcome::InsufficientFunds => {
                return Ok(BuyOutcome::InsufficientFunds {
                    available_zat: app.db.available_zat(tg_id)?,
                    needed_zat: order.total_zat(),
                });
            }
        }
    }
    // After the DEBIT, an error must never read as "nothing was bought": park it as UNCERTAIN.
    let order = reload(&app.db, order_id)?;
    match purchase(app, order.clone()).await {
        Ok(outcome) => Ok(outcome),
        Err(e) => {
            tracing::error!(error = %e, order_id, "purchase step failed after debit");
            let current = reload(&app.db, order_id)?;
            if current.status == "PURCHASING" {
                mark_uncertain(app, &current, "internal error after debit")
            } else {
                Ok(BuyOutcome::Uncertain(current))
            }
        }
    }
}

/// Order is PURCHASING with its DEBIT written. Never retries the purchase.
async fn purchase(app: &App, order: Order) -> anyhow::Result<BuyOutcome> {
    let operator =
        Operator::parse(&order.operator).ok_or_else(|| anyhow::anyhow!("bad operator"))?;
    let package = match app.bitrefill.get_product(slug_for(operator)).await {
        Ok(product) => pick_package(&product, order.inr).cloned(),
        Err(e) => {
            tracing::warn!(error = %e, order_id = %order.id, "catalog lookup before invoice failed");
            None
        }
    };
    let Some(package) = package else {
        return fail_and_refund(app, &order, "package unavailable at purchase time").await;
    };
    let invoice = app
        .bitrefill
        .create_invoice(&order.product_slug, &package.value, &order.phone, &order.id)
        .await;
    let invoice = match invoice {
        Ok(id) => id,
        Err(bitrefill::Error::DryRun) => {
            fail_and_refund(app, &order, "dry run: no invoice created").await?;
            return Ok(BuyOutcome::DryRun(reload(&app.db, &order.id)?));
        }
        // Rejected before any invoice exists: definitely not charged.
        Err(
            e @ (bitrefill::Error::Http {
                status: 400..=499, ..
            }
            | bitrefill::Error::RateLimited),
        ) => {
            return fail_and_refund(app, &order, &e.to_string()).await;
        }
        // Timeout, 5xx, transport: an invoice may exist. Never retry; a human resolves it.
        Err(e) => return mark_uncertain(app, &order, &e.to_string()),
    };
    app.db.set_invoice(&order.id, &invoice.0, now_ts())?;
    let deadline = tokio::time::Instant::now() + POLL_FOR;
    while tokio::time::Instant::now() < deadline {
        match app.bitrefill.get_invoice(&invoice).await {
            Ok(InvoiceStatus::Delivered) => {
                advance(&app.db, &order, Event::Deliver, now_ts())?;
                return Ok(BuyOutcome::Delivered(reload(&app.db, &order.id)?));
            }
            Ok(InvoiceStatus::Failed) => {
                return fail_and_refund(app, &order, "bitrefill invoice failed").await;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, order_id = %order.id, "invoice poll failed"),
        }
        tokio::time::sleep(POLL_EVERY).await;
    }
    mark_uncertain(app, &order, "not delivered within 15 s; still checking")
}

/// External error text (Bitrefill bodies) may echo the request; never store or log the phone.
fn scrub_phone(text: &str, phone: &str) -> String {
    if phone.is_empty() {
        return text.to_owned();
    }
    text.replace(phone, "<phone>")
}

async fn fail_and_refund(app: &App, order: &Order, why: &str) -> anyhow::Result<BuyOutcome> {
    let why = &scrub_phone(why, &order.phone);
    let now = now_ts();
    app.db.set_error(&order.id, why, now)?;
    advance(&app.db, order, Event::Fail, now)?;
    app.db.reverse_order(&order.id, order.total_zat(), now)?;
    Ok(BuyOutcome::Failed(reload(&app.db, &order.id)?))
}

fn mark_uncertain(app: &App, order: &Order, why: &str) -> anyhow::Result<BuyOutcome> {
    let why = &scrub_phone(why, &order.phone);
    let now = now_ts();
    app.db.set_error(&order.id, why, now)?;
    advance(&app.db, order, Event::MarkUncertain, now)?;
    tracing::warn!(order_id = %order.id, why, "order UNCERTAIN; operator must resolve");
    Ok(BuyOutcome::Uncertain(reload(&app.db, &order.id)?))
}

/// ❌ tap. `true` if the order moved QUOTED → CANCELLED and belongs to `tg_id`.
pub fn cancel(app: &App, order_id: &str, tg_id: i64, now: i64) -> anyhow::Result<bool> {
    match app.db.get_order(order_id)? {
        Some(o) if o.tg_id == tg_id && o.status == "QUOTED" => {
            advance(&app.db, &o, Event::Cancel, now)
        }
        _ => Ok(false),
    }
}

/// UNCERTAIN orders with an invoice: one status check each. Delivered → DELIVERED; failed →
/// FAILED + refund; anything else stays for the next round. Returns orders that changed.
pub async fn resolve_uncertain(app: &App) -> anyhow::Result<Vec<Order>> {
    let mut changed = Vec::new();
    for order in app.db.orders_in_status("UNCERTAIN")? {
        let Some(inv) = order.bitrefill_invoice.clone() else {
            continue;
        };
        let event = match app.bitrefill.get_invoice(&InvoiceId(inv)).await {
            Ok(InvoiceStatus::Delivered) => Event::ResolveDelivered,
            Ok(InvoiceStatus::Failed) => Event::ResolveFailed,
            _ => continue,
        };
        let now = now_ts();
        if advance(&app.db, &order, event, now)? {
            if event == Event::ResolveFailed {
                app.db.reverse_order(&order.id, order.total_zat(), now)?;
            }
            changed.push(reload(&app.db, &order.id)?);
        }
    }
    Ok(changed)
}

/// Startup recovery (design §6): expire stale quotes; PURCHASING orders interrupted by a crash
/// become UNCERTAIN (those with an invoice resolve via `resolve_uncertain`). Returns orders
/// that need an operator alert.
pub async fn recover(app: &App, now: i64) -> anyhow::Result<Vec<Order>> {
    for o in app.db.orders_in_status("QUOTED")? {
        if now >= o.quote_expires_at {
            advance(&app.db, &o, Event::Expire, now)?;
        }
    }
    let mut alerts = Vec::new();
    for o in app.db.orders_in_status("PURCHASING")? {
        app.db.set_error(&o.id, "interrupted by restart", now)?;
        advance(&app.db, &o, Event::MarkUncertain, now)?;
        if o.bitrefill_invoice.is_none() {
            alerts.push(reload(&app.db, &o.id)?);
        }
    }
    Ok(alerts)
}

/// IST day window `[start, end)` containing `now` (UTC epoch seconds).
pub fn ist_day(now: i64) -> (i64, i64) {
    const IST_OFFSET: i64 = 5 * 3600 + 1800;
    const DAY: i64 = 86_400;
    let start = (now + IST_OFFSET).div_euclid(DAY) * DAY - IST_OFFSET;
    (start, start + DAY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn app(dry_run: bool) -> App {
        let env: HashMap<&str, &str> = [
            ("TELEGRAM_BOT_TOKEN", "t"),
            ("BITREFILL_API_KEY", "k"),
            ("ANTHROPIC_API_KEY", "a"),
            ("SSA_TREASURY_UA", "utest1treasury"),
            ("BITREFILL_DRY_RUN", if dry_run { "1" } else { "0" }),
        ]
        .into();
        let cfg = Config::from_lookup(|k| env.get(k).map(|v| v.to_string())).unwrap();
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        App {
            cfg,
            env_path: "/nonexistent/.env".into(),
            db: Arc::new(db),
            zec: Arc::new(Zec::new("/x".into(), "/x".into(), "/x".into(), false)),
            bitrefill: Bitrefill::new("http://127.0.0.1:9", "k", dry_run).unwrap(),
            prices: PriceCache::new().unwrap(),
        }
    }

    fn quoted(app: &App, id: &str, tg_id: i64, now: i64) {
        app.db
            .insert_quote(&NewOrder {
                id: id.into(),
                tg_id,
                operator: "jio".into(),
                product_slug: slug_for(Operator::Jio).into(),
                phone: "9876543210".into(),
                inr: 10,
                usd_cents: 12,
                price_zat: 9_000,
                fee_zat: FEE_ZAT,
                dry_run: true,
                quote_expires_at: now + QUOTE_TTL_SECS,
                created_at: now,
            })
            .unwrap();
    }

    fn member(app: &App, tg_id: i64, credit: i64) {
        app.db.add_invite("I", 10).unwrap();
        app.db.admit("I", tg_id, None, 0).unwrap();
        app.db
            .set_wallet(tg_id, "uuid", "uview", "utest1u")
            .unwrap();
        if credit > 0 {
            app.db
                .credit_deposit(tg_id, &format!("tx{tg_id}"), credit, 0)
                .unwrap();
        }
    }

    #[tokio::test]
    async fn not_owner_cannot_buy_and_nothing_is_debited() {
        let app = app(true);
        member(&app, 1, 1_000_000);
        member(&app, 2, 1_000_000);
        quoted(&app, "o1", 1, 100);
        let out = confirm_and_buy(&app, "o1", 2, 101).await.unwrap();
        assert!(matches!(out, BuyOutcome::NotOwner));
        assert_eq!(app.db.available_zat(1).unwrap(), 1_000_000);
        assert_eq!(app.db.get_order("o1").unwrap().unwrap().status, "QUOTED");
    }

    #[tokio::test]
    async fn insufficient_funds_reports_both_amounts() {
        let app = app(true);
        member(&app, 1, 10);
        quoted(&app, "o1", 1, 100);
        match confirm_and_buy(&app, "o1", 1, 101).await.unwrap() {
            BuyOutcome::InsufficientFunds {
                available_zat,
                needed_zat,
            } => {
                assert_eq!((available_zat, needed_zat), (10, 9_000 + FEE_ZAT))
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn expired_quote_is_refused_and_marked_expired() {
        let app = app(true);
        member(&app, 1, 1_000_000);
        quoted(&app, "o1", 1, 100);
        let out = confirm_and_buy(&app, "o1", 1, 100 + QUOTE_TTL_SECS)
            .await
            .unwrap();
        assert!(matches!(out, BuyOutcome::Expired));
        assert_eq!(app.db.get_order("o1").unwrap().unwrap().status, "EXPIRED");
    }

    #[tokio::test]
    async fn unreachable_bitrefill_refunds_once_and_double_tap_buys_nothing() {
        let app = app(true);
        member(&app, 1, 1_000_000);
        quoted(&app, "o1", 1, 100);
        let first = confirm_and_buy(&app, "o1", 1, 101).await.unwrap();
        assert!(matches!(first, BuyOutcome::Failed(_)), "{first:?}");
        let second = confirm_and_buy(&app, "o1", 1, 102).await.unwrap();
        // Refused by the 30 s limit (the failed attempt's DEBIT counts) or by NotQuoted: either
        // way the second tap buys nothing.
        assert!(
            matches!(
                second,
                BuyOutcome::NotQuoted | BuyOutcome::Refused(caps::Decision::TooSoon)
            ),
            "{second:?}"
        );
        let third = confirm_and_buy(&app, "o1", 1, 200).await.unwrap();
        assert!(matches!(third, BuyOutcome::NotQuoted), "{third:?}");
        assert_eq!(app.db.available_zat(1).unwrap(), 1_000_000);
        assert_eq!(app.db.get_order("o1").unwrap().unwrap().status, "FAILED");
    }

    #[test]
    fn cancel_only_for_owner_and_only_once() {
        let app = app(true);
        member(&app, 1, 0);
        member(&app, 2, 0);
        quoted(&app, "o1", 1, 100);
        assert!(!cancel(&app, "o1", 2, 101).unwrap());
        assert!(cancel(&app, "o1", 1, 101).unwrap());
        assert!(!cancel(&app, "o1", 1, 102).unwrap());
    }

    #[test]
    fn ist_day_boundaries() {
        // 2026-11-04 00:00 IST = 2026-11-03 18:30 UTC = 1793730600
        let midnight_ist = 1_793_730_600;
        assert_eq!(ist_day(midnight_ist), (midnight_ist, midnight_ist + 86_400));
        assert_eq!(ist_day(midnight_ist - 1).1, midnight_ist);
        assert_eq!(ist_day(midnight_ist + 86_399).0, midnight_ist);
    }

    #[test]
    fn pick_package_takes_cheapest_with_exact_inr() {
        let p = |v: &str, inr, price| Package {
            value: v.into(),
            inr,
            price,
        };
        let product = Product {
            slug: "s".into(),
            name: "n".into(),
            packages: vec![
                p("a", Some(10), 150),
                p("b", Some(10), 141),
                p("c", Some(20), 1),
                p("d", None, 1),
            ],
        };
        assert_eq!(pick_package(&product, 10).unwrap().value, "b");
        assert!(pick_package(&product, 15).is_none());
        assert_eq!(available_inr(&product), vec![10, 20]);
    }

    // ---- real purchase path (dry_run = false) against a local mock Bitrefill, no network ----

    use serde_json::json;
    use tokio::net::{TcpListener, TcpStream};

    const JIO_CATALOG: &str = include_str!("../fixtures/bitrefill/reliance-jio-india-bundles.json");
    const INVOICE_DELIVERED: &str =
        include_str!("../fixtures/bitrefill/invoice-delivered.synthetic.json");
    const INVOICE_ID: &str = "inv-test-1";
    const ROUTE_CATALOG: &str = "GET /products/reliance-jio-india-bundles";
    const ROUTE_CREATE: &str = "POST /invoices";
    const ROUTE_STATUS: &str = "GET /invoices/inv-test-1";

    /// (HTTP status, JSON body)
    type Canned = (u16, String);
    type Routes = Arc<std::sync::Mutex<HashMap<String, Vec<Canned>>>>;
    /// (`"METHOD /path"`, raw request) in arrival order.
    type Seen = Arc<std::sync::Mutex<Vec<(String, String)>>>;

    /// Hand-rolled HTTP/1.1 mock, one request per connection (`Connection: close`).
    /// Each route holds a script: every hit pops the next response, the last one repeats.
    // ponytail: exact "METHOD /path" matching only; enough for the 3 Bitrefill routes.
    struct MockBitrefill {
        base: String,
        routes: Routes,
        seen: Seen,
    }

    impl MockBitrefill {
        async fn start(routes: Vec<(&str, Vec<Canned>)>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let routes: Routes = Arc::new(std::sync::Mutex::new(
                routes
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            ));
            let seen: Seen = Arc::default();
            let (r, s) = (Arc::clone(&routes), Arc::clone(&seen));
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let (r, s) = (Arc::clone(&r), Arc::clone(&s));
                    tokio::spawn(async move {
                        let _ = mock_serve(stream, r, s).await;
                    });
                }
            });
            Self { base, routes, seen }
        }

        /// Replaces a route's script (e.g. flip the invoice to delivered mid-test).
        fn set(&self, route: &str, script: Vec<Canned>) {
            self.routes
                .lock()
                .unwrap()
                .insert(route.to_string(), script);
        }

        fn count(&self, route: &str) -> usize {
            self.seen
                .lock()
                .unwrap()
                .iter()
                .filter(|(k, _)| k == route)
                .count()
        }

        /// Raw requests (headers + body) seen on a route.
        fn requests(&self, route: &str) -> Vec<String> {
            self.seen
                .lock()
                .unwrap()
                .iter()
                .filter(|(k, _)| k == route)
                .map(|(_, raw)| raw.clone())
                .collect()
        }
    }

    fn next_canned(routes: &Routes, key: &str) -> Canned {
        let mut routes = routes.lock().unwrap();
        match routes.get_mut(key) {
            Some(script) if script.len() > 1 => script.remove(0),
            Some(script) => script
                .first()
                .cloned()
                .unwrap_or((404, r#"{"error":"empty script"}"#.into())),
            None => (404, r#"{"error":"no such route"}"#.into()),
        }
    }

    /// Reads headers, then `Content-Length` body bytes, so closing never RSTs an unread body.
    async fn mock_read_request(stream: &TcpStream) -> std::io::Result<String> {
        let mut buf = Vec::new();
        loop {
            if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&buf[..end]).to_lowercase();
                let len = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if buf.len() >= end + 4 + len {
                    break;
                }
            }
            stream.readable().await?;
            let mut chunk = [0u8; 4096];
            match stream.try_read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }
        }
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    async fn mock_write_all(stream: &TcpStream, mut bytes: &[u8]) -> std::io::Result<()> {
        while !bytes.is_empty() {
            stream.writable().await?;
            match stream.try_write(bytes) {
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    async fn mock_serve(stream: TcpStream, routes: Routes, seen: Seen) -> std::io::Result<()> {
        let raw = mock_read_request(&stream).await?;
        let key = raw.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
        seen.lock().unwrap().push((key.clone(), raw));
        let (status, body) = next_canned(&routes, &key);
        let response = format!(
            "HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        mock_write_all(&stream, response.as_bytes()).await
    }

    fn app_with_bitrefill(base: &str) -> App {
        let mut app = app(false);
        app.bitrefill = Bitrefill::new(base, "k", false).unwrap();
        app
    }

    fn ok(body: impl Into<String>) -> Canned {
        (200, body.into())
    }

    fn create_ok() -> Canned {
        ok(json!({"data": {"id": INVOICE_ID, "status": "unpaid"}}).to_string())
    }

    fn invoice_json(status: &str, order_status: &str) -> Canned {
        ok(json!({"data": {
            "id": INVOICE_ID, "status": status, "orders": [{"id": "o", "status": order_status}]
        }})
        .to_string())
    }

    fn pending() -> Canned {
        invoice_json("pending", "created")
    }

    fn delivered() -> Canned {
        ok(INVOICE_DELIVERED)
    }

    fn failed() -> Canned {
        invoice_json("complete", "failed")
    }

    /// Catalog + create + status routes; `status_script` drives `GET /invoices/inv-test-1`.
    async fn mock_with(create: Canned, status_script: Vec<Canned>) -> MockBitrefill {
        MockBitrefill::start(vec![
            (ROUTE_CATALOG, vec![ok(JIO_CATALOG)]),
            (ROUTE_CREATE, vec![create]),
            (ROUTE_STATUS, status_script),
        ])
        .await
    }

    fn total_zat() -> i64 {
        9_000 + FEE_ZAT
    }

    fn status_of(app: &App, id: &str) -> String {
        app.db.get_order(id).unwrap().unwrap().status
    }

    fn invoice_of(app: &App, id: &str) -> Option<String> {
        app.db.get_order(id).unwrap().unwrap().bitrefill_invoice
    }

    /// Parked in PURCHASING with a DEBIT and an invoice id, then UNCERTAIN (as a timed-out
    /// poll leaves it), without waiting 15 s.
    fn seed_uncertain(app: &App, id: &str, tg_id: i64, invoice: &str) {
        quoted(app, id, tg_id, 100);
        let confirmed = app.db.confirm_order(id, tg_id, total_zat(), 101).unwrap();
        assert!(matches!(confirmed, ConfirmOutcome::Confirmed));
        app.db.set_invoice(id, invoice, 102).unwrap();
        let order = app.db.get_order(id).unwrap().unwrap();
        assert!(advance(&app.db, &order, Event::MarkUncertain, 103).unwrap());
    }

    #[tokio::test]
    async fn real_purchase_delivers_and_debits_once() {
        let mock = mock_with(create_ok(), vec![pending(), delivered()]).await;
        let app = app_with_bitrefill(&mock.base);
        member(&app, 1, 1_000_000);
        quoted(&app, "o1", 1, 100);

        let out = confirm_and_buy(&app, "o1", 1, 101).await.unwrap();

        let order = match out {
            BuyOutcome::Delivered(o) => o,
            other => panic!("{other:?}"),
        };
        assert_eq!(order.status, "DELIVERED");
        assert_eq!(order.bitrefill_invoice.as_deref(), Some(INVOICE_ID));
        assert_eq!(status_of(&app, "o1"), "DELIVERED");
        assert_eq!(app.db.available_zat(1).unwrap(), 1_000_000 - total_zat());
        assert_eq!(mock.count(ROUTE_CREATE), 1);
        assert_eq!(mock.count(ROUTE_STATUS), 2, "pending, then delivered");

        // What actually went on the wire: catalog package id, balance + auto_pay, bearer key.
        let raw = mock.requests(ROUTE_CREATE).remove(0);
        assert!(
            raw.to_lowercase().contains("authorization: bearer k"),
            "{raw}"
        );
        let body: serde_json::Value =
            serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["payment_method"], "balance");
        assert_eq!(body["auto_pay"], true);
        assert_eq!(
            body["products"][0]["package_id"],
            "reliance-jio-india-bundles<&>₹7.47 Talktime Top-Up"
        );
    }

    #[tokio::test]
    async fn invoice_reported_failed_refunds_the_budget() {
        let mock = mock_with(create_ok(), vec![failed()]).await;
        let app = app_with_bitrefill(&mock.base);
        member(&app, 1, 1_000_000);
        quoted(&app, "o1", 1, 100);

        let out = confirm_and_buy(&app, "o1", 1, 101).await.unwrap();

        assert!(matches!(out, BuyOutcome::Failed(_)), "{out:?}");
        assert_eq!(status_of(&app, "o1"), "FAILED");
        assert_eq!(app.db.available_zat(1).unwrap(), 1_000_000);
        assert_eq!(mock.count(ROUTE_CREATE), 1);
    }

    #[tokio::test]
    async fn create_invoice_400_fails_and_refunds_with_no_invoice() {
        let mock = mock_with((400, r#"{"error":"bad package"}"#.into()), vec![pending()]).await;
        let app = app_with_bitrefill(&mock.base);
        member(&app, 1, 1_000_000);
        quoted(&app, "o1", 1, 100);

        let out = confirm_and_buy(&app, "o1", 1, 101).await.unwrap();

        assert!(matches!(out, BuyOutcome::Failed(_)), "{out:?}");
        assert_eq!(status_of(&app, "o1"), "FAILED");
        assert_eq!(app.db.available_zat(1).unwrap(), 1_000_000);
        assert_eq!(mock.count(ROUTE_CREATE), 1);
        assert_eq!(invoice_of(&app, "o1"), None, "no invoice was created");
        assert_eq!(mock.count(ROUTE_STATUS), 0, "nothing to poll");
    }

    #[tokio::test]
    async fn create_invoice_500_is_uncertain_keeps_the_debit_and_is_never_retried() {
        let mock = mock_with((500, r#"{"error":"boom"}"#.into()), vec![delivered()]).await;
        let app = app_with_bitrefill(&mock.base);
        member(&app, 1, 1_000_000);
        quoted(&app, "o1", 1, 100);

        let out = confirm_and_buy(&app, "o1", 1, 101).await.unwrap();

        assert!(matches!(out, BuyOutcome::Uncertain(_)), "{out:?}");
        assert_eq!(status_of(&app, "o1"), "UNCERTAIN");
        // Unknown outcome: never auto-refund.
        assert_eq!(app.db.available_zat(1).unwrap(), 1_000_000 - total_zat());
        assert_eq!(mock.count(ROUTE_CREATE), 1);

        // Second tap, well past the 30 s limit, so the order-state guard (not TooSoon) decides.
        let again = confirm_and_buy(&app, "o1", 1, 1_000).await.unwrap();
        assert!(matches!(again, BuyOutcome::NotQuoted), "{again:?}");
        assert_eq!(mock.count(ROUTE_CREATE), 1, "no second POST");
        assert_eq!(app.db.available_zat(1).unwrap(), 1_000_000 - total_zat());

        // The 500 gave us no invoice id, so resolve_uncertain has nothing to poll: the order
        // stays UNCERTAIN for the operator (startup `recover` alerts on these) even though the
        // mock would now say delivered.
        assert_eq!(invoice_of(&app, "o1"), None);
        assert!(resolve_uncertain(&app).await.unwrap().is_empty());
        assert_eq!(status_of(&app, "o1"), "UNCERTAIN");
        assert_eq!(mock.count(ROUTE_STATUS), 0);
        assert_eq!(mock.count(ROUTE_CREATE), 1);
    }

    /// Real time: the poll window is 15 s (`POLL_FOR`) and `tokio::time::pause` needs tokio's
    /// `test-util` feature, which Cargo.toml does not enable. Other tests run in parallel, so
    /// this only adds ~15 s of wall clock. `resolve_uncertain` covers the faster paths.
    #[tokio::test]
    async fn invoice_pending_past_poll_window_is_uncertain_then_resolves_delivered() {
        let mock = mock_with(create_ok(), vec![pending()]).await;
        let app = app_with_bitrefill(&mock.base);
        member(&app, 1, 1_000_000);
        quoted(&app, "o1", 1, 100);

        let out = confirm_and_buy(&app, "o1", 1, 101).await.unwrap();

        assert!(matches!(out, BuyOutcome::Uncertain(_)), "{out:?}");
        assert_eq!(status_of(&app, "o1"), "UNCERTAIN");
        assert_eq!(invoice_of(&app, "o1").as_deref(), Some(INVOICE_ID));
        assert_eq!(app.db.available_zat(1).unwrap(), 1_000_000 - total_zat());
        assert_eq!(mock.count(ROUTE_CREATE), 1);
        assert!(mock.count(ROUTE_STATUS) >= 10, "polled about once a second");

        // Still pending: one status check, no change.
        assert!(resolve_uncertain(&app).await.unwrap().is_empty());
        assert_eq!(status_of(&app, "o1"), "UNCERTAIN");

        // Bitrefill now reports delivered: the next round settles it.
        mock.set(ROUTE_STATUS, vec![delivered()]);
        let changed = resolve_uncertain(&app).await.unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!(status_of(&app, "o1"), "DELIVERED");
        assert_eq!(app.db.available_zat(1).unwrap(), 1_000_000 - total_zat());
        assert_eq!(mock.count(ROUTE_CREATE), 1, "resolving never POSTs");
    }

    #[tokio::test]
    async fn resolve_uncertain_settles_delivered_and_refunds_failed() {
        let mock = MockBitrefill::start(vec![
            ("GET /invoices/inv-a", vec![delivered()]),
            ("GET /invoices/inv-b", vec![failed()]),
        ])
        .await;
        let app = app_with_bitrefill(&mock.base);
        member(&app, 1, 1_000_000);
        member(&app, 2, 1_000_000);
        seed_uncertain(&app, "oa", 1, "inv-a");
        seed_uncertain(&app, "ob", 2, "inv-b");
        assert_eq!(app.db.available_zat(2).unwrap(), 1_000_000 - total_zat());

        let changed = resolve_uncertain(&app).await.unwrap();

        assert_eq!(changed.len(), 2);
        assert_eq!(status_of(&app, "oa"), "DELIVERED");
        assert_eq!(app.db.available_zat(1).unwrap(), 1_000_000 - total_zat());
        assert_eq!(status_of(&app, "ob"), "FAILED");
        assert_eq!(app.db.available_zat(2).unwrap(), 1_000_000, "refunded");

        // Idempotent: nothing is UNCERTAIN any more, balances do not move.
        assert!(resolve_uncertain(&app).await.unwrap().is_empty());
        assert_eq!(app.db.available_zat(2).unwrap(), 1_000_000);
        assert_eq!(mock.count(ROUTE_CREATE), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn double_tap_posts_one_invoice_and_debits_once() {
        let mock = mock_with(create_ok(), vec![delivered()]).await;
        let app = app_with_bitrefill(&mock.base);
        member(&app, 1, 1_000_000);
        quoted(&app, "o1", 1, 100);

        let (a, b) = tokio::join!(
            confirm_and_buy(&app, "o1", 1, 101),
            confirm_and_buy(&app, "o1", 1, 101)
        );

        let outcomes = [a.unwrap(), b.unwrap()];
        let bought = outcomes
            .iter()
            .filter(|o| matches!(o, BuyOutcome::Delivered(_)))
            .count();
        let turned_away = outcomes
            .iter()
            .filter(|o| {
                matches!(
                    o,
                    BuyOutcome::NotQuoted | BuyOutcome::Refused(caps::Decision::TooSoon)
                )
            })
            .count();
        assert_eq!((bought, turned_away), (1, 1), "{outcomes:?}");
        assert_eq!(mock.count(ROUTE_CREATE), 1);
        // Two DEBITs would leave 1_000_000 - 2 * total_zat().
        assert_eq!(app.db.available_zat(1).unwrap(), 1_000_000 - total_zat());
        assert_eq!(status_of(&app, "o1"), "DELIVERED");
    }
}
