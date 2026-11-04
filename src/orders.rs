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
        let decision = caps_for(&app.db, tg_id, operator, order.inr, now)?;
        if decision != caps::Decision::Allow {
            return Ok(BuyOutcome::Refused(decision));
        }
        match app
            .db
            .confirm_order(order_id, tg_id, order.total_zat(), now)?
        {
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
    purchase(app, reload(&app.db, order_id)?).await
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

async fn fail_and_refund(app: &App, order: &Order, why: &str) -> anyhow::Result<BuyOutcome> {
    let now = now_ts();
    app.db.set_error(&order.id, why, now)?;
    advance(&app.db, order, Event::Fail, now)?;
    app.db.reverse_order(&order.id, order.total_zat(), now)?;
    Ok(BuyOutcome::Failed(reload(&app.db, &order.id)?))
}

fn mark_uncertain(app: &App, order: &Order, why: &str) -> anyhow::Result<BuyOutcome> {
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
}
