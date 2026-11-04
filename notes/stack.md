# Stack versions (P0.9): checked on crates.io 2026-10-30

Candidate versions for hack day. Nothing is pinned in a Cargo.toml yet; that file gets written on 4 Nov.

| Crate | Version | Use | Note |
|---|---|---|---|
| tokio | 1.54.1 | async runtime | features: `rt-multi-thread`, `macros`, `process`, `sync`, `time`, `net` |
| teloxide | 0.17.0 | Telegram bot | `macros` feature; check which reqwest version it pulls in |
| reqwest | 0.13.5 | Bitrefill, Claude, NEAR | `json`, `rustls-tls`; if teloxide pins 0.12, use 0.12 too so only one copy compiles |
| rusqlite | 0.41.1 | app DB | `bundled` feature (no system SQLite dependency) |
| serde / serde_json | 1.0.229 / 1.0.154 | JSON | `derive` |
| thiserror / anyhow | 2.0.21 / 1.0.104 | errors | per ADR / standards §1 |
| tracing / tracing-subscriber | 0.1.44 / 0.3.23 | logs | `env-filter` |
| uuid | 1.29.0 | order ids | `v4` |
| chrono | 0.4.45 | IST day boundaries | `FixedOffset::east_opt(5*3600+1800)`, so no tz database crate is needed |
| qrcode | 0.14.1 | deposit QR | with `image` |
| image | 0.25.10 | PNG encode for QR | `default-features = false`, `png` |
| dotenvy | 0.15.7 | `.env` loading | |
| axum | check in P0.12 | `/stats.json` (FR-16) | must share hyper 1.x with reqwest; tokio needs `net` |

Toolchain: rustc/cargo 1.99.0 (local). zcash-devtool pinned at commit `0b3436b` (17 Oct 2026).

**On hack day, first 10 min:** `cargo new`, add these deps, run `cargo build` once to warm the cache.
**Before then** (allowed prep): a throwaway `cargo build` of an empty crate with these deps, *outside*
the repo (e.g. `~/Zcash/tools/depcheck`), to fill the cargo cache and catch version conflicts.
That needs Bash permission to compile (see Phase 0 notes).
