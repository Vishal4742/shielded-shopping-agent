//! SQLite persistence (docs/05 §1.1, §3.2). All SQL is parameterized; money is `i64`
//! (zat, rupees, cents); timestamps are UTC epoch seconds passed in by the caller.
//!
//! Order statuses are plain `&str` (`"QUOTED"`, `"PURCHASING"`, ...) so this module does not
//! depend on `order_fsm`; callers validate transitions there first.

use std::fmt;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result, bail, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

const MIGRATION_001: &str = include_str!("../migrations/001_init.sql");
const BUSY_TIMEOUT_MS: u64 = 5_000;
/// BR-5: order states that count toward spend caps.
const COUNTED_STATUSES: &str = "'PURCHASING','UNCERTAIN','DELIVERED','RECEIPT_SENDING','RECEIPTED'";

/// Shared handle: one connection behind a mutex, so `Arc<Db>` works across tasks.
pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct User {
    pub tg_id: i64,
    pub tg_username: Option<String>,
    pub account_uuid: Option<String>,
    pub ufvk: Option<String>,
    pub deposit_ua: Option<String>,
    pub created_at: i64,
}

impl fmt::Debug for User {
    /// UFVK is never printed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("User")
            .field("tg_id", &self.tg_id)
            .field("tg_username", &self.tg_username)
            .field("account_uuid", &self.account_uuid)
            .field("ufvk", &"<redacted>")
            .field("deposit_ua", &self.deposit_ua)
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// A freshly quoted order; `insert_quote` stores it with status `QUOTED`.
#[derive(Clone, PartialEq, Eq)]
pub struct NewOrder {
    pub id: String,
    pub tg_id: i64,
    pub operator: String,
    pub product_slug: String,
    pub phone: String,
    pub inr: i64,
    pub usd_cents: i64,
    pub price_zat: i64,
    pub fee_zat: i64,
    pub dry_run: bool,
    pub quote_expires_at: i64,
    pub created_at: i64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Order {
    pub id: String,
    pub tg_id: i64,
    pub operator: String,
    pub product_slug: String,
    pub phone: String,
    pub inr: i64,
    pub usd_cents: i64,
    pub price_zat: i64,
    pub fee_zat: i64,
    pub status: String,
    pub dry_run: bool,
    pub bitrefill_invoice: Option<String>,
    pub receipt_txid: Option<String>,
    pub error: Option<String>,
    pub quote_expires_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Order {
    pub fn total_zat(&self) -> i64 {
        self.price_zat + self.fee_zat
    }
}

/// Keeps only the last 2 digits: full phone numbers must never reach logs.
fn mask_phone(phone: &str) -> String {
    let tail: String = phone
        .chars()
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("********{tail}")
}

impl fmt::Debug for Order {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Order")
            .field("id", &self.id)
            .field("tg_id", &self.tg_id)
            .field("operator", &self.operator)
            .field("phone", &mask_phone(&self.phone))
            .field("inr", &self.inr)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for NewOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewOrder")
            .field("id", &self.id)
            .field("tg_id", &self.tg_id)
            .field("operator", &self.operator)
            .field("phone", &mask_phone(&self.phone))
            .field("inr", &self.inr)
            .finish_non_exhaustive()
    }
}

/// Result of [`Db::confirm_order`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmOutcome {
    /// Order is now `PURCHASING` and a DEBIT was written.
    Confirmed,
    /// Order does not exist, or belongs to another user.
    NotOwner,
    /// Order exists but is not `QUOTED` (double tap, cancelled, already purchasing...).
    NotQuoted,
    /// `now >= quote_expires_at`. The order is left `QUOTED`; the caller moves it to `EXPIRED`.
    Expired,
    /// Ledger balance is below `total_zat`.
    InsufficientFunds,
}

const ORDER_COLS: &str = "id, tg_id, operator, product_slug, phone, inr, usd_cents, price_zat, \
     fee_zat, status, dry_run, bitrefill_invoice, receipt_txid, error, quote_expires_at, \
     created_at, updated_at";

fn order_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Order> {
    Ok(Order {
        id: r.get(0)?,
        tg_id: r.get(1)?,
        operator: r.get(2)?,
        product_slug: r.get(3)?,
        phone: r.get(4)?,
        inr: r.get(5)?,
        usd_cents: r.get(6)?,
        price_zat: r.get(7)?,
        fee_zat: r.get(8)?,
        status: r.get(9)?,
        dry_run: r.get::<_, i64>(10)? != 0,
        bitrefill_invoice: r.get(11)?,
        receipt_txid: r.get(12)?,
        error: r.get(13)?,
        quote_expires_at: r.get(14)?,
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
    })
}

/// Σ CREDIT − Σ DEBIT + Σ REVERSAL (FR-5). Works on a connection or a transaction.
fn available(conn: &Connection, tg_id: i64) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COALESCE(SUM(CASE kind WHEN 'DEBIT' THEN -zat ELSE zat END), 0) \
         FROM ledger WHERE tg_id = ?1",
        [tg_id],
        |r| r.get(0),
    )
}

impl Db {
    /// Opens (creating parent dirs) a file DB with WAL and foreign keys on. Does not migrate.
    pub fn open(path: impl AsRef<Path>) -> Result<Db> {
        let path = path.as_ref();
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("creating db directory {}", dir.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening database {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::configure(conn)
    }

    /// In-memory DB for tests. Does not migrate.
    pub fn open_in_memory() -> Result<Db> {
        Self::configure(Connection::open_in_memory()?)
    }

    fn configure(conn: Connection) -> Result<Db> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS))?;
        Ok(Db {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        // A panic while holding the lock cannot leave SQLite half-written, so recover.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Applies `migrations/001_init.sql` (idempotent).
    pub fn migrate(&self) -> Result<()> {
        self.lock()
            .execute_batch(MIGRATION_001)
            .context("applying migration 001")
    }

    // ---- invites / users ----

    /// Admits `tg_id` with an invite `code`. Atomic: decrements `uses_left` (only if > 0) and
    /// inserts a placeholder user row. Returns `true` if the user is a member afterwards; an
    /// existing user is `true` and consumes nothing. `false` = unknown or exhausted code.
    pub fn admit(&self, code: &str, tg_id: i64, username: Option<&str>, now: i64) -> Result<bool> {
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists = tx
            .query_row("SELECT 1 FROM users WHERE tg_id = ?1", [tg_id], |_| Ok(()))
            .optional()?
            .is_some();
        if exists {
            return Ok(true);
        }
        let used = tx.execute(
            "UPDATE invites SET uses_left = uses_left - 1 WHERE code = ?1 AND uses_left > 0",
            [code],
        )?;
        if used != 1 {
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO users (tg_id, tg_username, created_at) VALUES (?1, ?2, ?3)",
            params![tg_id, username, now],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn is_user(&self, tg_id: i64) -> Result<bool> {
        Ok(self
            .lock()
            .query_row("SELECT 1 FROM users WHERE tg_id = ?1", [tg_id], |_| Ok(()))
            .optional()?
            .is_some())
    }

    /// Stores the devtool account for an admitted user. Errors if the user does not exist.
    pub fn set_wallet(
        &self,
        tg_id: i64,
        account_uuid: &str,
        ufvk: &str,
        deposit_ua: &str,
    ) -> Result<()> {
        let n = self.lock().execute(
            "UPDATE users SET account_uuid = ?2, ufvk = ?3, deposit_ua = ?4 WHERE tg_id = ?1",
            params![tg_id, account_uuid, ufvk, deposit_ua],
        )?;
        ensure!(n == 1, "set_wallet: no such user");
        Ok(())
    }

    pub fn get_user(&self, tg_id: i64) -> Result<Option<User>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT tg_id, tg_username, account_uuid, ufvk, deposit_ua, created_at \
                 FROM users WHERE tg_id = ?1",
                [tg_id],
                |r| {
                    Ok(User {
                        tg_id: r.get(0)?,
                        tg_username: r.get(1)?,
                        account_uuid: r.get(2)?,
                        ufvk: r.get(3)?,
                        deposit_ua: r.get(4)?,
                        created_at: r.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    // ---- ledger ----

    /// Records a deposit. `true` if newly inserted, `false` if the txid was already credited.
    pub fn credit_deposit(&self, tg_id: i64, txid: &str, zat: i64, now: i64) -> Result<bool> {
        let n = self.lock().execute(
            // ON CONFLICT (not OR IGNORE): the latter would also swallow CHECK failures (zat <= 0).
            "INSERT INTO ledger (tg_id, kind, zat, txid, created_at) \
             VALUES (?1, 'CREDIT', ?2, ?3, ?4) ON CONFLICT DO NOTHING",
            params![tg_id, zat, txid, now],
        )?;
        Ok(n == 1)
    }

    pub fn available_zat(&self, tg_id: i64) -> Result<i64> {
        Ok(available(&self.lock(), tg_id)?)
    }

    /// Writes a REVERSAL for a failed order. Idempotent via `UNIQUE(kind, order_id)`:
    /// `true` if written now, `false` if it already existed. Errors if the order is unknown.
    pub fn reverse_order(&self, order_id: &str, zat: i64, now: i64) -> Result<bool> {
        let conn = self.lock();
        let Some(tg_id) = conn
            .query_row("SELECT tg_id FROM orders WHERE id = ?1", [order_id], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
        else {
            bail!("reverse_order: unknown order");
        };
        let n = conn.execute(
            "INSERT INTO ledger (tg_id, kind, zat, order_id, created_at) \
             VALUES (?1, 'REVERSAL', ?2, ?3, ?4) ON CONFLICT DO NOTHING",
            params![tg_id, zat, order_id, now],
        )?;
        Ok(n == 1)
    }

    // ---- orders ----

    pub fn insert_quote(&self, o: &NewOrder) -> Result<()> {
        self.lock().execute(
            "INSERT INTO orders (id, tg_id, operator, product_slug, phone, inr, usd_cents, \
             price_zat, fee_zat, status, dry_run, quote_expires_at, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'QUOTED', ?10, ?11, ?12, ?12)",
            params![
                o.id,
                o.tg_id,
                o.operator,
                o.product_slug,
                o.phone,
                o.inr,
                o.usd_cents,
                o.price_zat,
                o.fee_zat,
                i64::from(o.dry_run),
                o.quote_expires_at,
                o.created_at
            ],
        )?;
        Ok(())
    }

    pub fn get_order(&self, id: &str) -> Result<Option<Order>> {
        Ok(self
            .lock()
            .query_row(
                &format!("SELECT {ORDER_COLS} FROM orders WHERE id = ?1"),
                [id],
                order_from_row,
            )
            .optional()?)
    }

    /// Conditional `UPDATE ... WHERE id=? AND status=from`; `true` iff exactly one row changed.
    /// The caller validates the edge with `order_fsm` first.
    pub fn transition(&self, id: &str, from: &str, to: &str, now: i64) -> Result<bool> {
        let n = self.lock().execute(
            "UPDATE orders SET status = ?3, updated_at = ?4 WHERE id = ?1 AND status = ?2",
            params![id, from, to, now],
        )?;
        Ok(n == 1)
    }

    /// FR-8 / docs/05 §3.2: in ONE `BEGIN IMMEDIATE` transaction check owner, `QUOTED`, not
    /// expired and funds, then move the order to `PURCHASING` and write the DEBIT.
    ///
    /// The caller must already have checked the pause switch (`config::paused`) and the caps
    /// (`caps::check`) before calling; this function does neither. `total_zat` must equal the
    /// order's `price_zat + fee_zat` (an error otherwise, so a bug cannot under-debit).
    pub fn confirm_order(
        &self,
        id: &str,
        tg_id: i64,
        total_zat: i64,
        now: i64,
    ) -> Result<ConfirmOutcome> {
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row = tx
            .query_row(
                "SELECT tg_id, status, quote_expires_at, price_zat + fee_zat FROM orders \
                 WHERE id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((owner, status, expires_at, order_total)) = row else {
            return Ok(ConfirmOutcome::NotOwner);
        };
        if owner != tg_id {
            return Ok(ConfirmOutcome::NotOwner);
        }
        if status != "QUOTED" {
            return Ok(ConfirmOutcome::NotQuoted);
        }
        if now >= expires_at {
            return Ok(ConfirmOutcome::Expired);
        }
        ensure!(
            total_zat == order_total,
            "confirm_order: total_zat does not match the order"
        );
        if available(&tx, tg_id)? < total_zat {
            return Ok(ConfirmOutcome::InsufficientFunds);
        }
        let changed = tx.execute(
            "UPDATE orders SET status = 'PURCHASING', updated_at = ?2 \
             WHERE id = ?1 AND status = 'QUOTED'",
            params![id, now],
        )?;
        ensure!(
            changed == 1,
            "confirm_order: conditional update changed {changed} rows"
        );
        tx.execute(
            "INSERT INTO ledger (tg_id, kind, zat, order_id, created_at) \
             VALUES (?1, 'DEBIT', ?2, ?3, ?4)",
            params![tg_id, total_zat, id, now],
        )?;
        tx.commit()?;
        Ok(ConfirmOutcome::Confirmed)
    }

    /// Rupees committed in `[day_start, day_end)` by `orders.created_at` (BR-5 states only,
    /// real orders only: `dry_run = 0`). `tg_id = None` sums across all users.
    pub fn spent_today_inr(&self, tg_id: Option<i64>, day_start: i64, day_end: i64) -> Result<i64> {
        let sql = format!(
            "SELECT COALESCE(SUM(inr), 0) FROM orders WHERE dry_run = 0 \
             AND status IN ({COUNTED_STATUSES}) AND created_at >= ?1 AND created_at < ?2 \
             AND (?3 IS NULL OR tg_id = ?3)"
        );
        Ok(self
            .lock()
            .query_row(&sql, params![day_start, day_end, tg_id], |r| r.get(0))?)
    }

    /// Time of the user's latest confirmed purchase (newest DEBIT), for BR-4 `TooSoon`.
    pub fn last_purchase_at(&self, tg_id: i64) -> Result<Option<i64>> {
        Ok(self.lock().query_row(
            "SELECT MAX(created_at) FROM ledger WHERE tg_id = ?1 AND kind = 'DEBIT'",
            [tg_id],
            |r| r.get(0),
        )?)
    }
}

/// Aggregates for `GET /stats.json` (FR-16). No per-user data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Stats {
    pub orders_delivered: i64,
    pub inr_spent_today: i64,
    pub receipts_sent: i64,
}

/// Helpers for the order flow, workers and stats (added on hack day by zcash-3f).
impl Db {
    pub fn add_invite(&self, code: &str, uses: i64) -> Result<()> {
        self.lock().execute(
            "INSERT INTO invites (code, uses_left) VALUES (?1, ?2) \
             ON CONFLICT(code) DO UPDATE SET uses_left = excluded.uses_left",
            params![code, uses],
        )?;
        Ok(())
    }

    /// Users that already have a wallet account (sync worker input).
    pub fn users_with_wallet(&self) -> Result<Vec<User>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT tg_id, tg_username, account_uuid, ufvk, deposit_ua, created_at \
             FROM users WHERE account_uuid IS NOT NULL ORDER BY tg_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(User {
                tg_id: r.get(0)?,
                tg_username: r.get(1)?,
                account_uuid: r.get(2)?,
                ufvk: r.get(3)?,
                deposit_ua: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Orders in `status`, oldest first.
    pub fn orders_in_status(&self, status: &str) -> Result<Vec<Order>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {ORDER_COLS} FROM orders WHERE status = ?1 ORDER BY created_at, id"
        ))?;
        let rows = stmt.query_map([status], order_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_invoice(&self, id: &str, invoice: &str, now: i64) -> Result<()> {
        self.lock().execute(
            "UPDATE orders SET bitrefill_invoice = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, invoice, now],
        )?;
        Ok(())
    }

    /// Last error for an order. Callers pass messages without secrets or phone numbers.
    pub fn set_error(&self, id: &str, error: &str, now: i64) -> Result<()> {
        self.lock().execute(
            "UPDATE orders SET error = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, error, now],
        )?;
        Ok(())
    }

    pub fn set_receipt_txid(&self, id: &str, txid: &str, now: i64) -> Result<()> {
        self.lock().execute(
            "UPDATE orders SET receipt_txid = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, txid, now],
        )?;
        Ok(())
    }

    pub fn stats(&self, day_start: i64, day_end: i64) -> Result<Stats> {
        let conn = self.lock();
        let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0));
        let orders_delivered = count(
            "SELECT COUNT(*) FROM orders WHERE dry_run = 0 \
             AND status IN ('DELIVERED','RECEIPT_SENDING','RECEIPTED')",
        )?;
        let receipts_sent = count("SELECT COUNT(*) FROM orders WHERE status = 'RECEIPTED'")?;
        drop(conn);
        Ok(Stats {
            orders_delivered,
            inr_spent_today: self.spent_today_inr(None, day_start, day_end)?,
            receipts_sent,
        })
    }
}

/// A recurring auto-pay rule. Approved once by the user; every run still goes through caps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: i64,
    pub tg_id: i64,
    pub operator: String,
    pub phone: String,
    pub inr: i64,
    pub every_secs: i64,
    pub next_run_at: i64,
    pub approved: bool,
    pub active: bool,
    pub skip_next: bool,
    pub warned_for: i64,
    pub last_order_id: Option<String>,
}

const RULE_COLS: &str = "id, tg_id, operator, phone, inr, every_secs, next_run_at, approved, \
     active, skip_next, warned_for, last_order_id";

fn rule_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Rule> {
    Ok(Rule {
        id: r.get(0)?,
        tg_id: r.get(1)?,
        operator: r.get(2)?,
        phone: r.get(3)?,
        inr: r.get(4)?,
        every_secs: r.get(5)?,
        next_run_at: r.get(6)?,
        approved: r.get::<_, i64>(7)? != 0,
        active: r.get::<_, i64>(8)? != 0,
        skip_next: r.get::<_, i64>(9)? != 0,
        warned_for: r.get(10)?,
        last_order_id: r.get(11)?,
    })
}

/// Auto-pay rules.
impl Db {
    /// New rule awaiting the user's ✅. Returns its id.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_rule(
        &self,
        tg_id: i64,
        operator: &str,
        phone: &str,
        inr: i64,
        every_secs: i64,
        first_run_at: i64,
        now: i64,
    ) -> Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO rules (tg_id, operator, phone, inr, every_secs, next_run_at, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![tg_id, operator, phone, inr, every_secs, first_run_at, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_rule(&self, id: i64) -> Result<Option<Rule>> {
        Ok(self
            .lock()
            .query_row(
                &format!("SELECT {RULE_COLS} FROM rules WHERE id = ?1"),
                [id],
                rule_from_row,
            )
            .optional()?)
    }

    /// ✅ on the rule card. Owner-checked; `true` iff it was pending and is now approved.
    pub fn approve_rule(&self, id: i64, tg_id: i64) -> Result<bool> {
        let n = self.lock().execute(
            "UPDATE rules SET approved = 1 WHERE id = ?1 AND tg_id = ?2 AND approved = 0 AND active = 1",
            params![id, tg_id],
        )?;
        Ok(n == 1)
    }

    /// Owner-checked stop. `true` iff it was active.
    pub fn stop_rule(&self, id: i64, tg_id: i64) -> Result<bool> {
        let n = self.lock().execute(
            "UPDATE rules SET active = 0 WHERE id = ?1 AND tg_id = ?2 AND active = 1",
            params![id, tg_id],
        )?;
        Ok(n == 1)
    }

    /// Owner-checked: skip the next run only.
    pub fn skip_next_run(&self, id: i64, tg_id: i64) -> Result<bool> {
        let n = self.lock().execute(
            "UPDATE rules SET skip_next = 1 WHERE id = ?1 AND tg_id = ?2 AND active = 1 AND approved = 1",
            params![id, tg_id],
        )?;
        Ok(n == 1)
    }

    pub fn active_rules(&self, tg_id: i64) -> Result<Vec<Rule>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {RULE_COLS} FROM rules WHERE tg_id = ?1 AND active = 1 AND approved = 1 ORDER BY id"
        ))?;
        let rows = stmt.query_map([tg_id], rule_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Approved, active rules whose next run is at or before `until`.
    pub fn rules_due_by(&self, until: i64) -> Result<Vec<Rule>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {RULE_COLS} FROM rules WHERE active = 1 AND approved = 1 AND next_run_at <= ?1 \
             ORDER BY next_run_at, id"
        ))?;
        let rows = stmt.query_map([until], rule_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn mark_rule_warned(&self, id: i64, run_at: i64) -> Result<()> {
        self.lock().execute(
            "UPDATE rules SET warned_for = ?2 WHERE id = ?1",
            params![id, run_at],
        )?;
        Ok(())
    }

    /// After a run (or skip): schedule the next one, clear the skip flag, remember the order.
    /// Conditional on `from_run_at` so two loops can't both run the same slot.
    pub fn advance_rule(
        &self,
        id: i64,
        from_run_at: i64,
        next_run_at: i64,
        last_order_id: Option<&str>,
    ) -> Result<bool> {
        let n = self.lock().execute(
            "UPDATE rules SET next_run_at = ?3, skip_next = 0, \
             last_order_id = COALESCE(?4, last_order_id) WHERE id = ?1 AND next_run_at = ?2",
            params![id, from_run_at, next_run_at, last_order_id],
        )?;
        Ok(n == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_need_owner_approval_and_advance_once_per_slot() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        db.add_invite("I", 5).unwrap();
        db.admit("I", 1, None, 0).unwrap();
        db.admit("I", 2, None, 0).unwrap();
        let id = db
            .insert_rule(1, "jio", "9876543210", 10, 120, 1000, 900)
            .unwrap();
        assert!(
            db.rules_due_by(5000).unwrap().is_empty(),
            "unapproved rules never run"
        );
        assert!(!db.approve_rule(id, 2).unwrap(), "only the owner approves");
        assert!(db.approve_rule(id, 1).unwrap());
        assert!(!db.approve_rule(id, 1).unwrap(), "approve once");
        assert_eq!(db.rules_due_by(999).unwrap().len(), 0);
        assert_eq!(db.rules_due_by(1000).unwrap().len(), 1);
        assert!(db.advance_rule(id, 1000, 1120, Some("o1")).unwrap());
        assert!(
            !db.advance_rule(id, 1000, 1120, Some("o2")).unwrap(),
            "slot already taken"
        );
        assert_eq!(
            db.get_rule(id).unwrap().unwrap().last_order_id.as_deref(),
            Some("o1")
        );
        assert!(!db.skip_next_run(id, 2).unwrap());
        assert!(db.skip_next_run(id, 1).unwrap());
        assert!(db.get_rule(id).unwrap().unwrap().skip_next);
        assert!(!db.stop_rule(id, 2).unwrap());
        assert!(db.stop_rule(id, 1).unwrap());
        assert!(db.rules_due_by(i64::MAX).unwrap().is_empty());
        assert!(db.active_rules(1).unwrap().is_empty());
    }

    #[test]
    fn hack_day_helpers_round_trip() {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        db.add_invite("DEMO1", 2).unwrap();
        assert!(db.admit("DEMO1", 7, None, 1).unwrap());
        assert!(db.users_with_wallet().unwrap().is_empty());
        db.set_wallet(7, "uuid-7", "uview", "utest1x").unwrap();
        assert_eq!(
            db.users_with_wallet().unwrap()[0].account_uuid.as_deref(),
            Some("uuid-7")
        );
        let o = NewOrder {
            id: "o1".into(),
            tg_id: 7,
            operator: "jio".into(),
            product_slug: "s".into(),
            phone: "9876543210".into(),
            inr: 10,
            usd_cents: 11,
            price_zat: 9000,
            fee_zat: 25000,
            dry_run: false,
            quote_expires_at: 600,
            created_at: 1,
        };
        db.insert_quote(&o).unwrap();
        db.set_invoice("o1", "inv-1", 2).unwrap();
        db.set_receipt_txid("o1", "tx", 3).unwrap();
        db.set_error("o1", "boom", 4).unwrap();
        let got = db.get_order("o1").unwrap().unwrap();
        assert_eq!(
            (
                got.bitrefill_invoice.as_deref(),
                got.receipt_txid.as_deref(),
                got.error.as_deref()
            ),
            (Some("inv-1"), Some("tx"), Some("boom"))
        );
        assert_eq!(db.orders_in_status("QUOTED").unwrap().len(), 1);
        assert!(db.transition("o1", "QUOTED", "RECEIPTED", 5).unwrap());
        assert_eq!(
            db.stats(0, 100).unwrap(),
            Stats {
                orders_delivered: 1,
                inr_spent_today: 10,
                receipts_sent: 1
            }
        );
    }

    const NOW: i64 = 1_000_000;
    const USER: i64 = 7;

    fn db() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.migrate().unwrap();
        db.lock()
            .execute(
                "INSERT INTO invites (code, uses_left) VALUES ('INVITE01', 2)",
                [],
            )
            .unwrap();
        assert!(db.admit("INVITE01", USER, Some("alice"), NOW).unwrap());
        db
    }

    fn quote(id: &str, tg_id: i64, inr: i64, dry_run: bool) -> NewOrder {
        NewOrder {
            id: id.into(),
            tg_id,
            operator: "jio".into(),
            product_slug: "reliance-jio-india-bundles".into(),
            phone: "9876543210".into(),
            inr,
            usd_cents: 120,
            price_zat: 90_000,
            fee_zat: 10_000,
            dry_run,
            quote_expires_at: NOW + 600,
            created_at: NOW,
        }
    }

    fn debit_count(db: &Db) -> i64 {
        db.lock()
            .query_row(
                "SELECT COUNT(*) FROM ledger WHERE kind = 'DEBIT'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn migrate_twice_is_fine() {
        let db = db();
        db.migrate().unwrap();
    }

    #[test]
    fn file_open_creates_parent_dir_and_wal() {
        let dir = std::env::temp_dir().join(format!("ssa-db-{}", uuid::Uuid::new_v4()));
        let db = Db::open(dir.join("nested/ssa.sqlite")).unwrap();
        db.migrate().unwrap();
        let mode: String = db
            .lock()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn invite_uses_left_hits_zero_and_existing_user_is_free() {
        let db = db(); // alice used 1 of 2
        assert!(!db.is_user(8).unwrap());
        assert!(db.admit("INVITE01", 8, None, NOW).unwrap());
        assert!(db.is_user(8).unwrap());
        assert!(!db.admit("INVITE01", 9, None, NOW).unwrap(), "exhausted");
        assert!(!db.is_user(9).unwrap());
        assert!(!db.admit("NOSUCH", 10, None, NOW).unwrap(), "unknown code");
        assert!(
            db.admit("INVITE01", USER, None, NOW).unwrap(),
            "member re-entering is free"
        );
        let left: i64 = db
            .lock()
            .query_row(
                "SELECT uses_left FROM invites WHERE code = 'INVITE01'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn set_wallet_round_trips_and_debug_hides_ufvk() {
        let db = db();
        db.set_wallet(USER, "uuid-1", "uviewtest1secret", "utest1deposit")
            .unwrap();
        let u = db.get_user(USER).unwrap().unwrap();
        assert_eq!(u.tg_username.as_deref(), Some("alice"));
        assert_eq!(u.account_uuid.as_deref(), Some("uuid-1"));
        assert_eq!(u.ufvk.as_deref(), Some("uviewtest1secret"));
        assert_eq!(u.deposit_ua.as_deref(), Some("utest1deposit"));
        assert!(!format!("{u:?}").contains("uviewtest1secret"));
        assert!(db.get_user(999).unwrap().is_none());
        assert!(db.set_wallet(999, "u", "k", "a").is_err());
    }

    #[test]
    fn double_credit_of_same_txid_inserts_once() {
        let db = db();
        assert!(db.credit_deposit(USER, "tx1", 500_000, NOW).unwrap());
        assert!(!db.credit_deposit(USER, "tx1", 500_000, NOW + 1).unwrap());
        assert_eq!(db.available_zat(USER).unwrap(), 500_000);
    }

    #[test]
    fn available_is_credits_minus_debits_plus_reversals() {
        let db = db();
        assert_eq!(db.available_zat(USER).unwrap(), 0);
        db.credit_deposit(USER, "tx1", 500_000, NOW).unwrap();
        db.credit_deposit(USER, "tx2", 250_000, NOW).unwrap();
        db.insert_quote(&quote("o1", USER, 100, false)).unwrap();
        assert_eq!(
            db.confirm_order("o1", USER, 100_000, NOW).unwrap(),
            ConfirmOutcome::Confirmed
        );
        assert_eq!(db.available_zat(USER).unwrap(), 650_000);
        assert!(db.reverse_order("o1", 100_000, NOW).unwrap());
        assert_eq!(db.available_zat(USER).unwrap(), 750_000);
    }

    #[test]
    fn reverse_order_is_idempotent_and_rejects_unknown_order() {
        let db = db();
        db.credit_deposit(USER, "tx1", 500_000, NOW).unwrap();
        db.insert_quote(&quote("o1", USER, 100, false)).unwrap();
        db.confirm_order("o1", USER, 100_000, NOW).unwrap();
        assert!(db.reverse_order("o1", 100_000, NOW).unwrap());
        assert!(!db.reverse_order("o1", 100_000, NOW + 1).unwrap());
        assert_eq!(db.available_zat(USER).unwrap(), 500_000);
        assert!(db.reverse_order("ghost", 1, NOW).is_err());
    }

    #[test]
    fn ledger_rejects_non_positive_zat_and_bad_kind() {
        let db = db();
        assert!(db.credit_deposit(USER, "tx0", 0, NOW).is_err());
        assert!(db.credit_deposit(USER, "txneg", -5, NOW).is_err());
        let bad = db.lock().execute(
            "INSERT INTO ledger (tg_id, kind, zat, created_at) VALUES (?1, 'BONUS', 1, ?2)",
            params![USER, NOW],
        );
        assert!(bad.is_err());
    }

    #[test]
    fn insert_and_get_order_round_trip() {
        let db = db();
        db.insert_quote(&quote("o1", USER, 100, true)).unwrap();
        let o = db.get_order("o1").unwrap().unwrap();
        assert_eq!(o.status, "QUOTED");
        assert!(o.dry_run);
        assert_eq!(o.total_zat(), 100_000);
        assert_eq!(o.quote_expires_at, NOW + 600);
        assert!(o.bitrefill_invoice.is_none());
        assert!(db.get_order("nope").unwrap().is_none());
        assert!(
            !format!("{o:?}").contains("9876543210"),
            "phone is masked in Debug"
        );
    }

    #[test]
    fn transition_is_conditional() {
        let db = db();
        db.insert_quote(&quote("o1", USER, 100, false)).unwrap();
        assert!(!db.transition("o1", "PURCHASING", "DELIVERED", NOW).unwrap());
        assert!(db.transition("o1", "QUOTED", "CANCELLED", NOW + 5).unwrap());
        assert!(!db.transition("o1", "QUOTED", "CANCELLED", NOW + 6).unwrap());
        let o = db.get_order("o1").unwrap().unwrap();
        assert_eq!((o.status.as_str(), o.updated_at), ("CANCELLED", NOW + 5));
        assert!(!db.transition("ghost", "QUOTED", "CANCELLED", NOW).unwrap());
    }

    #[test]
    fn confirm_happy_path_moves_to_purchasing_and_writes_one_debit() {
        let db = db();
        db.credit_deposit(USER, "tx1", 500_000, NOW).unwrap();
        db.insert_quote(&quote("o1", USER, 100, false)).unwrap();
        assert_eq!(
            db.confirm_order("o1", USER, 100_000, NOW + 1).unwrap(),
            ConfirmOutcome::Confirmed
        );
        assert_eq!(db.get_order("o1").unwrap().unwrap().status, "PURCHASING");
        assert_eq!(db.available_zat(USER).unwrap(), 400_000);
        assert_eq!(db.last_purchase_at(USER).unwrap(), Some(NOW + 1));
    }

    #[test]
    fn double_confirm_fails_the_second_time_with_one_debit() {
        let db = db();
        db.credit_deposit(USER, "tx1", 500_000, NOW).unwrap();
        db.insert_quote(&quote("o1", USER, 100, false)).unwrap();
        assert_eq!(
            db.confirm_order("o1", USER, 100_000, NOW).unwrap(),
            ConfirmOutcome::Confirmed
        );
        assert_eq!(
            db.confirm_order("o1", USER, 100_000, NOW).unwrap(),
            ConfirmOutcome::NotQuoted
        );
        assert_eq!(debit_count(&db), 1);
        assert_eq!(db.available_zat(USER).unwrap(), 400_000);
    }

    #[test]
    fn confirm_refuses_expired_insufficient_and_foreign_orders() {
        let db = db();
        assert!(db.admit("INVITE01", 8, None, NOW).unwrap());
        db.credit_deposit(USER, "tx1", 500_000, NOW).unwrap();
        db.insert_quote(&quote("o1", USER, 100, false)).unwrap();

        assert_eq!(
            db.confirm_order("o1", 8, 100_000, NOW).unwrap(),
            ConfirmOutcome::NotOwner
        );
        assert_eq!(
            db.confirm_order("ghost", USER, 1, NOW).unwrap(),
            ConfirmOutcome::NotOwner
        );
        assert_eq!(
            db.confirm_order("o1", USER, 100_000, NOW + 600).unwrap(),
            ConfirmOutcome::Expired,
            "expiry is exclusive: now == quote_expires_at is expired"
        );
        assert_eq!(db.get_order("o1").unwrap().unwrap().status, "QUOTED");

        db.insert_quote(&NewOrder {
            fee_zat: 1_000_000,
            ..quote("o2", USER, 100, false)
        })
        .unwrap();
        assert_eq!(
            db.confirm_order("o2", USER, 1_090_000, NOW).unwrap(),
            ConfirmOutcome::InsufficientFunds
        );
        assert_eq!(db.get_order("o2").unwrap().unwrap().status, "QUOTED");

        assert!(
            db.confirm_order("o1", USER, 5, NOW).is_err(),
            "mismatched total is a bug"
        );
        db.transition("o1", "QUOTED", "CANCELLED", NOW).unwrap();
        assert_eq!(
            db.confirm_order("o1", USER, 100_000, NOW).unwrap(),
            ConfirmOutcome::NotQuoted
        );
        assert_eq!(debit_count(&db), 0);
    }

    #[test]
    fn spent_today_excludes_dry_run_cancelled_quoted_and_other_days() {
        let db = db();
        assert!(db.admit("INVITE01", 8, None, NOW).unwrap());
        let day = (NOW, NOW + 86_400);
        let mk = |id: &str, tg: i64, inr: i64, dry: bool, status: &str, created: i64| {
            db.insert_quote(&NewOrder {
                created_at: created,
                ..quote(id, tg, inr, dry)
            })
            .unwrap();
            db.lock()
                .execute(
                    "UPDATE orders SET status = ?2 WHERE id = ?1",
                    params![id, status],
                )
                .unwrap();
        };
        mk("a", USER, 100, false, "PURCHASING", NOW);
        mk("b", USER, 200, false, "DELIVERED", NOW + 10);
        mk("c", USER, 400, false, "CANCELLED", NOW + 20);
        mk("d", USER, 800, true, "DELIVERED", NOW + 30);
        mk("e", USER, 1600, false, "QUOTED", NOW + 40);
        mk("f", USER, 3200, false, "FAILED", NOW + 50);
        mk("g", USER, 6400, false, "RECEIPTED", NOW - 1); // yesterday
        mk("h", 8, 12_800, false, "UNCERTAIN", NOW + 60);
        mk("i", 8, 25_600, false, "RECEIPT_SENDING", NOW + 70);

        assert_eq!(db.spent_today_inr(Some(USER), day.0, day.1).unwrap(), 300);
        assert_eq!(db.spent_today_inr(Some(8), day.0, day.1).unwrap(), 38_400);
        assert_eq!(db.spent_today_inr(None, day.0, day.1).unwrap(), 38_700);
        assert_eq!(db.spent_today_inr(Some(99), day.0, day.1).unwrap(), 0);
    }

    #[test]
    fn last_purchase_at_is_none_without_debits() {
        assert_eq!(db().last_purchase_at(USER).unwrap(), None);
    }
}
