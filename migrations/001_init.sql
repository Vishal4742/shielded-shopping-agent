-- Schema per docs/05-detailed-design.md §1.1. Idempotent: safe to apply on every start.
-- Money is integers only (zat, rupees, cents). Timestamps are UTC epoch seconds.

CREATE TABLE IF NOT EXISTS invites (
    code      TEXT PRIMARY KEY,
    uses_left INTEGER NOT NULL CHECK (uses_left >= 0)
);

CREATE TABLE IF NOT EXISTS users (
    tg_id        INTEGER PRIMARY KEY,
    tg_username  TEXT,
    account_uuid TEXT,
    ufvk         TEXT,
    deposit_ua   TEXT,
    created_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS orders (
    id                TEXT PRIMARY KEY,
    tg_id             INTEGER NOT NULL REFERENCES users(tg_id),
    operator          TEXT NOT NULL,
    product_slug      TEXT NOT NULL,
    phone             TEXT NOT NULL,
    inr               INTEGER NOT NULL,
    usd_cents         INTEGER NOT NULL,
    price_zat         INTEGER NOT NULL,
    fee_zat           INTEGER NOT NULL,
    status            TEXT NOT NULL,
    dry_run           INTEGER NOT NULL CHECK (dry_run IN (0, 1)),
    bitrefill_invoice TEXT,
    receipt_txid      TEXT,
    error             TEXT,
    quote_expires_at  INTEGER NOT NULL,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_orders_tg_created ON orders(tg_id, created_at);
CREATE INDEX IF NOT EXISTS idx_orders_status ON orders(status);

CREATE TABLE IF NOT EXISTS ledger (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    tg_id      INTEGER NOT NULL REFERENCES users(tg_id),
    kind       TEXT NOT NULL CHECK (kind IN ('CREDIT', 'DEBIT', 'REVERSAL')),
    zat        INTEGER NOT NULL CHECK (zat > 0),
    txid       TEXT UNIQUE,                 -- deposits only; SQLite allows many NULLs
    order_id   TEXT REFERENCES orders(id),  -- debits/reversals
    created_at INTEGER NOT NULL,
    UNIQUE (kind, order_id)
);
