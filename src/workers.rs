//! Background workers: deposit sync (FR-4, design §3.5), receipt queue (FR-10, §3.4) and the
//! public `GET /stats.json` server (FR-16, §10.4).
//!
//! Each loop starts its next pass only after the current one finishes, and never crashes on an
//! error: it logs and tries again next round. Logs carry tg_id, zat and order ids only, never
//! keys, phone numbers or memos.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};

use crate::config;
use crate::db::{Order, Stats};
use crate::order_fsm::{self, Event, Status};
use crate::orders::{App, ist_day};
use crate::zec::{Tx, ZecError, find_memo_txid};

const SYNC_EVERY: Duration = Duration::from_secs(20);
const RECEIPT_EVERY: Duration = Duration::from_secs(15);
const HTTP_IO_TIMEOUT: Duration = Duration::from_secs(5);
/// Most of a request we ever read; the request line is all we need.
const HTTP_MAX_REQUEST: usize = 8 * 1024;

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

// ---------------------------------------------------------------------------------------------
// Sync worker: credit deposits
// ---------------------------------------------------------------------------------------------

/// Per-account crediting decision. Returns `(txid, zat)` for every *mined* tx that paid
/// `account_uuid` a positive amount in non-change outputs. `list-tx` prints a tx once per wallet
/// account it touches, so a txid is counted once.
///
/// Wallet-internal transfers (user → treasury receipts) are received by the treasury, never by a
/// user account, so filtering on `received_by == account_uuid` already excludes them.
// ponytail: `Tx` has no "sent from" field; if treasury→user transfers must not credit, add it to
// `zec::TxOutput` and filter here.
pub fn deposits_for(account_uuid: &str, txs: &[Tx]) -> Vec<(String, i64)> {
    let mut seen = HashSet::new();
    txs.iter()
        .filter(|tx| tx.mined_height.is_some() && seen.insert(tx.txid.as_str()))
        .filter_map(|tx| {
            let zat: i64 = tx
                .outputs
                .iter()
                .filter(|o| !o.is_change && o.received_by.as_deref() == Some(account_uuid))
                .map(|o| o.value_zat)
                .sum();
            (zat > 0).then(|| (tx.txid.clone(), zat))
        })
        .collect()
}

/// Every 20 s: sync the wallet, then credit new deposits for each user account.
pub async fn sync_loop(app: Arc<App>) {
    loop {
        if let Err(e) = sync_pass(&app).await {
            tracing::warn!(error = %e, "sync pass failed");
        }
        tokio::time::sleep(SYNC_EVERY).await;
    }
}

async fn sync_pass(app: &App) -> anyhow::Result<()> {
    if !app.zec.try_sync().await? {
        return Ok(()); // wallet busy (a send is running); skip this round
    }
    for user in app.db.users_with_wallet()? {
        let Some(uuid) = user.account_uuid.as_deref() else {
            continue;
        };
        let txs = match app.zec.list_tx(uuid).await {
            Ok(txs) => txs,
            Err(e) => {
                tracing::warn!(tg_id = user.tg_id, error = %e, "list-tx failed");
                continue;
            }
        };
        for (txid, zat) in deposits_for(uuid, &txs) {
            match app.db.credit_deposit(user.tg_id, &txid, zat, now_ts()) {
                Ok(true) => tracing::info!(tg_id = user.tg_id, zat, "deposit credited"),
                Ok(false) => {}
                Err(e) => tracing::warn!(tg_id = user.tg_id, error = %e, "credit_deposit failed"),
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Receipt queue
// ---------------------------------------------------------------------------------------------

/// What to do next for an order in RECEIPT_SENDING.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptStep {
    /// The receipt tx exists and is mined: record the txid and finish.
    MarkReceipted(String),
    /// Dry-run order: no real purchase, so no real receipt. Finish without a send.
    SkipDryRun,
    /// A receipt tx exists (or was already sent) but is not mined yet.
    Wait,
    /// No receipt tx anywhere: send one.
    Send,
}

/// The receipt tx for `order` among the user's txs: by memo (order id) first, then by the txid we
/// recorded after a send. The second lookup stops a re-send if the memo is not shown for some reason.
fn locate_receipt_tx<'a>(txs: &'a [Tx], order: &Order) -> Option<&'a Tx> {
    let by_memo = find_memo_txid(txs, &order.id);
    let txid = by_memo.or(order.receipt_txid.as_deref())?;
    txs.iter().find(|tx| tx.txid == txid)
}

/// Pure decision. Never returns `Send` for a dry-run order or one we already sent.
pub fn next_receipt_step(order: &Order, found: Option<&Tx>) -> ReceiptStep {
    if order.dry_run {
        return ReceiptStep::SkipDryRun;
    }
    match found {
        Some(tx) if tx.mined_height.is_some() => ReceiptStep::MarkReceipted(tx.txid.clone()),
        Some(_) => ReceiptStep::Wait,
        // Sent earlier but not listed yet: wait rather than risk a second receipt.
        // ponytail: stuck forever if the tx is dropped; the operator sees the order and resets it.
        None if order.receipt_txid.is_some() => ReceiptStep::Wait,
        None => ReceiptStep::Send,
    }
}

fn receipt_memo(order: &Order) -> String {
    format!("SSA·{}·{}·₹{}", order.id, order.operator, order.inr)
}

/// `order_fsm` edge + conditional UPDATE. `false` if the order was no longer in `from`.
fn advance(app: &App, order_id: &str, from: Status, event: Event) -> anyhow::Result<bool> {
    let to = order_fsm::apply(from, event)?;
    app.db.transition(
        order_id,
        order_fsm::as_str(from),
        order_fsm::as_str(to),
        now_ts(),
    )
}

/// Every 15 s: start the oldest DELIVERED order's receipt, then work every RECEIPT_SENDING order.
pub async fn receipt_loop(app: Arc<App>) {
    loop {
        if let Err(e) = receipt_pass(&app).await {
            tracing::warn!(error = %e, "receipt pass failed");
        }
        tokio::time::sleep(RECEIPT_EVERY).await;
    }
}

async fn receipt_pass(app: &App) -> anyhow::Result<()> {
    if let Some(oldest) = app.db.orders_in_status("DELIVERED")?.first() {
        advance(app, &oldest.id, Status::Delivered, Event::StartReceipt)?;
    }
    for order in app.db.orders_in_status("RECEIPT_SENDING")? {
        // One bad order must not block the queue behind it.
        if let Err(e) = process_receipt(app, &order).await {
            tracing::warn!(order = %order.id, error = %e, "receipt step failed");
        }
    }
    Ok(())
}

async fn process_receipt(app: &App, order: &Order) -> anyhow::Result<()> {
    let account = app
        .db
        .get_user(order.tg_id)?
        .and_then(|u| u.account_uuid)
        .ok_or_else(|| anyhow::anyhow!("order {} has no wallet account", order.id))?;
    let txs = if order.dry_run {
        Vec::new()
    } else {
        app.zec.list_tx(&account).await?
    };
    let found = locate_receipt_tx(&txs, order);
    if let Some(tx) = found
        && order.receipt_txid.as_deref() != Some(tx.txid.as_str())
    {
        app.db.set_receipt_txid(&order.id, &tx.txid, now_ts())?;
    }
    match next_receipt_step(order, found) {
        ReceiptStep::MarkReceipted(_) => {
            advance(app, &order.id, Status::ReceiptSending, Event::ReceiptMined)?;
            tracing::info!(order = %order.id, "receipt confirmed");
        }
        ReceiptStep::SkipDryRun => {
            advance(app, &order.id, Status::ReceiptSending, Event::ReceiptMined)?;
            tracing::info!(order = %order.id, "dry-run order: receipt skipped");
        }
        ReceiptStep::Wait => {}
        ReceiptStep::Send => send_receipt(app, order, &account).await?,
    }
    Ok(())
}

async fn send_receipt(app: &App, order: &Order, account: &str) -> anyhow::Result<()> {
    let sent = app
        .zec
        .send(
            account,
            &app.cfg.treasury_ua,
            order.price_zat,
            &receipt_memo(order),
        )
        .await;
    match sent {
        Ok(txid) => {
            app.db.set_receipt_txid(&order.id, &txid, now_ts())?;
            tracing::info!(order = %order.id, "receipt sent");
        }
        Err(ZecError::InsufficientFunds) => {
            tracing::info!(order = %order.id, "receipt waits for spendable funds");
        }
        // A timeout may still have broadcast; the next round finds it by memo before any re-send.
        Err(e) => {
            app.db
                .set_error(&order.id, "receipt send failed", now_ts())?;
            return Err(e.into());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// GET /stats.json
// ---------------------------------------------------------------------------------------------

/// True for `GET /stats.json` (query string ignored). Reads only the request line.
fn is_stats_request(request: &str) -> bool {
    let mut parts = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let (method, target) = (parts.next(), parts.next());
    method == Some("GET") && target.and_then(|t| t.split('?').next()) == Some("/stats.json")
}

fn http_response(status: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn stats_response(stats: &Stats, paused: bool) -> String {
    let body = format!(
        "{{\"orders_delivered\":{},\"inr_spent_today\":{},\"receipts_sent\":{},\"paused\":{paused}}}",
        stats.orders_delivered, stats.inr_spent_today, stats.receipts_sent
    );
    http_response("200 OK", "application/json", &body)
}

/// Pure router: `stats` is only called for `GET /stats.json`; anything else is a 404.
fn build_response(request: &str, stats: impl FnOnce() -> anyhow::Result<(Stats, bool)>) -> String {
    if !is_stats_request(request) {
        return http_response("404 Not Found", "text/plain", "not found");
    }
    match stats() {
        Ok((s, paused)) => stats_response(&s, paused),
        Err(e) => {
            tracing::warn!(error = %e, "stats query failed");
            http_response("500 Internal Server Error", "text/plain", "error")
        }
    }
}

/// Reads up to `HTTP_MAX_REQUEST` bytes, stopping at the blank line that ends the headers.
// ponytail: hand-rolled HTTP for one GET route; switch to axum if routes grow.
// (`readable`/`try_read` keep us on tokio's `net` feature, no `io-util` needed.)
async fn read_request(stream: &TcpStream) -> std::io::Result<String> {
    let mut buf = Vec::with_capacity(1024);
    while buf.len() < HTTP_MAX_REQUEST && !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        stream.readable().await?;
        let mut chunk = [0u8; 1024];
        let room = (HTTP_MAX_REQUEST - buf.len()).min(chunk.len());
        match stream.try_read(&mut chunk[..room]) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

async fn write_all(stream: &TcpStream, mut bytes: &[u8]) -> std::io::Result<()> {
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

async fn serve_connection(app: Arc<App>, stream: TcpStream) -> std::io::Result<()> {
    let request = read_request(&stream).await?;
    let response = build_response(&request, || {
        let (start, end) = ist_day(now_ts());
        Ok((app.db.stats(start, end)?, config::paused(&app.env_path)))
    });
    write_all(&stream, response.as_bytes()).await
}

/// Serves `GET /stats.json` on `addr` until the process exits. A bind failure is logged and ends
/// only this task.
const MAX_STATS_CONNECTIONS: usize = 32;

pub async fn stats_server(app: Arc<App>, addr: SocketAddr) {
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(%addr, error = %e, "stats server could not bind");
            return;
        }
    };
    tracing::info!(%addr, "stats server listening");
    // It's public via the tunnel: bound concurrent connections; excess ones are dropped.
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_STATS_CONNECTIONS));
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                tracing::warn!(error = %e, "stats accept failed");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        let Ok(permit) = Arc::clone(&slots).try_acquire_owned() else {
            drop(stream);
            continue;
        };
        let app = Arc::clone(&app);
        tokio::spawn(async move {
            let _permit = permit;
            match tokio::time::timeout(HTTP_IO_TIMEOUT, serve_connection(app, stream)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::debug!(error = %e, "stats connection error"),
                Err(_) => tracing::debug!("stats connection timed out"),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zec::{TxOutput, parse_list_tx};

    const LIST_TX_MEMO: &str = include_str!("../fixtures/devtool/list-tx-memo.txt");
    const TREASURY: &str = "093fb250-5e57-4ca4-a371-ce280b15b7be";
    const USER_TEST: &str = "d87ee4a6-ea65-4258-b903-713b7ba04ede";
    const FAUCET_TXID: &str = "752751150345164f641417ef6c02160c969e03818b25ee71b37174ab306b54ca";
    const TREASURY_TXID: &str = "3a999d11cf252c8469253e18760cac2de18398803cc91b00b077fe95f24a8a90";

    #[test]
    fn user_gets_faucet_tx_and_nothing_for_internal_transfer() {
        let txs = parse_list_tx(LIST_TX_MEMO).unwrap();
        assert_eq!(
            deposits_for(USER_TEST, &txs),
            vec![(FAUCET_TXID.to_string(), 100_000_000)]
        );
    }

    #[test]
    fn treasury_gets_its_faucet_tx_first() {
        let txs = parse_list_tx(LIST_TX_MEMO).unwrap();
        let deposits = deposits_for(TREASURY, &txs);
        assert_eq!(deposits[0], (TREASURY_TXID.to_string(), 100_000_000));
        // The fixture's 5c5a… transfer is printed twice (once per account) and counted once.
        // Treasury is not a user account, so the sync loop never credits it.
        assert_eq!(deposits.len(), 2);
    }

    fn tx(txid: &str, mined: Option<i64>, outs: Vec<(i64, bool, Option<&str>)>) -> Tx {
        Tx {
            txid: txid.into(),
            mined_height: mined,
            outputs: outs
                .into_iter()
                .map(|(value_zat, is_change, by)| TxOutput {
                    value_zat,
                    is_change,
                    received_by: by.map(str::to_owned),
                    memo: None,
                })
                .collect(),
        }
    }

    #[test]
    fn deposits_skip_unmined_change_and_other_accounts_and_dedupe() {
        let twice = tx(
            "d",
            Some(1),
            vec![(3, false, Some("u")), (4, false, Some("u"))],
        );
        let txs = vec![
            tx("a", None, vec![(5, false, Some("u"))]),
            tx("b", Some(1), vec![(7, true, Some("u"))]),
            tx("c", Some(1), vec![(9, false, Some("other"))]),
            twice.clone(),
            twice,
        ];
        assert_eq!(deposits_for("u", &txs), vec![("d".to_string(), 7)]);
    }

    fn order(dry_run: bool, receipt_txid: Option<&str>) -> Order {
        Order {
            id: "oid".into(),
            tg_id: 1,
            operator: "jio".into(),
            product_slug: "p".into(),
            phone: "0".into(),
            inr: 10,
            usd_cents: 11,
            price_zat: 100_000,
            fee_zat: 25_000,
            status: "RECEIPT_SENDING".into(),
            dry_run,
            bitrefill_invoice: None,
            receipt_txid: receipt_txid.map(str::to_owned),
            error: None,
            quote_expires_at: 0,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn receipt_step_decisions() {
        let mined = tx("t", Some(5), vec![]);
        let pending = tx("t", None, vec![]);
        let live = order(false, None);
        assert_eq!(next_receipt_step(&live, None), ReceiptStep::Send);
        assert_eq!(next_receipt_step(&live, Some(&pending)), ReceiptStep::Wait);
        assert_eq!(
            next_receipt_step(&live, Some(&mined)),
            ReceiptStep::MarkReceipted("t".into())
        );
        // already sent, tx not listed yet: never send twice
        assert_eq!(
            next_receipt_step(&order(false, Some("t")), None),
            ReceiptStep::Wait
        );
        // dry run: never sends, whatever the wallet shows
        assert_eq!(
            next_receipt_step(&order(true, None), None),
            ReceiptStep::SkipDryRun
        );
        assert_eq!(
            next_receipt_step(&order(true, None), Some(&mined)),
            ReceiptStep::SkipDryRun
        );
    }

    #[test]
    fn locate_finds_by_memo_then_by_recorded_txid() {
        let mut with_memo = tx("m", Some(1), vec![(1, false, None)]);
        with_memo.outputs[0].memo = Some("SSA·oid·jio·₹10".into());
        let other = tx("r", Some(2), vec![]);
        let txs = vec![other.clone(), with_memo];
        assert_eq!(
            locate_receipt_tx(&txs, &order(false, None)).unwrap().txid,
            "m"
        );
        let txs = vec![other];
        assert_eq!(
            locate_receipt_tx(&txs, &order(false, Some("r")))
                .unwrap()
                .txid,
            "r"
        );
        assert!(locate_receipt_tx(&txs, &order(false, None)).is_none());
    }

    #[test]
    fn memo_format() {
        assert_eq!(receipt_memo(&order(false, None)), "SSA·oid·jio·₹10");
    }

    fn stats() -> anyhow::Result<(Stats, bool)> {
        Ok((
            Stats {
                orders_delivered: 3,
                inr_spent_today: 60,
                receipts_sent: 2,
            },
            true,
        ))
    }

    #[test]
    fn stats_route_returns_json_with_cors() {
        let r = build_response("GET /stats.json?x=1 HTTP/1.1\r\nHost: a\r\n\r\n", stats);
        let body = r#"{"orders_delivered":3,"inr_spent_today":60,"receipts_sent":2,"paused":true}"#;
        assert!(r.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(r.contains("Content-Type: application/json\r\n"));
        assert!(r.contains("Access-Control-Allow-Origin: *\r\n"));
        assert!(r.contains(&format!("Content-Length: {}\r\n", body.len())));
        assert!(r.ends_with(&format!("\r\n\r\n{body}")));
    }

    #[test]
    fn other_routes_and_methods_are_404_without_touching_the_db() {
        for req in [
            "GET / HTTP/1.1\r\n\r\n",
            "GET /stats.json.bak HTTP/1.1\r\n\r\n",
            "POST /stats.json HTTP/1.1\r\n\r\n",
            "",
        ] {
            let r = build_response(req, || panic!("stats must not be queried"));
            assert!(r.starts_with("HTTP/1.1 404 Not Found\r\n"), "{req:?}");
        }
    }

    #[test]
    fn stats_failure_is_500() {
        let r = build_response("GET /stats.json HTTP/1.1\r\n\r\n", || {
            Err(anyhow::anyhow!("db down"))
        });
        assert!(r.starts_with("HTTP/1.1 500 "));
        assert!(!r.contains("db down"));
    }
}
