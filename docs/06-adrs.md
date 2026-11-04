# 06 — Architecture Decision Records

Format: Context → Decision → Consequences. Status: **Accepted** unless noted. Date: 30 Oct 2026.

## ADR-001 Rust for all project code
- **Context:** Zcash's maintained libraries and devtool are Rust; the team prefers Rust.
- **Decision:** a single Rust binary (tokio, teloxide, reqwest, rusqlite).
- **Consequences:** + one language, strong types for money maths. − Slower compiles; no official Anthropic SDK (see ADR-007).

## ADR-002 zcash-devtool as a subprocess for wallet operations
- **Context:** In-process `zcash_client_backend` needs custom sync/send code (hundreds of lines). WebZjs, the Snap and Zallet are unusable (feasibility §3).
- **Decision:** call the devtool binary, parse its text/CSV output, and serialise access per wallet.
- **Consequences:** + proven, Ironwood/NU7-ready. − One process per operation; the CLI may change (pin the commit; output fixtures). Upgrade path: replace the `zec` module with in-process crates.

## ADR-003 One wallet, one ZIP-32 account per user *(revised 30 Oct after P0.5)*
- **Context:** A UFVK must reveal only one user's history. P0.5 confirmed that devtool supports several accounts per wallet (`generate-account`), each with its own UFVK, and account-scoped `balance`/`list-tx`/`send` (notes/devtool.md).
- **Decision:** a single wallet dir with account 0 = treasury and accounts 1..n = users (store the account UUID per user).
- **Consequences:** + one sync per pass, one lock, faster. + UFVK per account keeps the privacy boundary. − All users share one wallet lock, so operations are serialised (fine at demo scale). Superseded design: one wallet dir per user.

## ADR-004 Prefunded Bitrefill balance, not a live ZEC swap
- **Context:** Bitrefill doesn't accept ZEC. A NEAR 1Click ZEC→USDC swap takes about 7.6 min against a 15-minute invoice window.
- **Decision:** pay with `payment_method=balance`, `auto_pay=true`. The swap is the 30-day milestone.
- **Consequences:** + instant, reliable demo. − The operator is the counterparty; privacy covers the user↔merchant link but not operator custody (stated in the pitch).

## ADR-005 Internal ledger for budgets
- **Context:** On-chain spendable balance drops while change confirms after each receipt (review R-5).
- **Decision:** spending decisions use the SQLite ledger (deposit credits, order debits). On-chain sends are for receipts only.
- **Consequences:** + instant, consistent available balance. − The ledger and the chain can drift if receipts fail; the receipt queue retries and alerts the operator.

## ADR-006 The LLM extracts intent only; code buys
- **Context:** Prompt-injection risk, a 12.5 h budget, and latency (review R-9).
- **Decision:** one structured-output Claude call returns `{operator, phone, amount_inr}`. There is no tool loop and no buy tool. A regex fallback handles the strict form.
- **Consequences:** + safe, fast, testable. − Less "agentic". The pitch frames autonomy as rule-bound purchasing, and standing rules are on the roadmap. *Revisit* for the Bitrefill track story if time allows.

## ADR-007 Raw HTTP to the Anthropic Messages API
- **Context:** No official Rust SDK.
- **Decision:** `reqwest` POST with a JSON body fixed by one real request in Phase 0.
- **Consequences:** + no dependency risk. − We own the error handling (retry on 429/5xx, bounded).

## ADR-008 Testnet ZEC with real Bitrefill purchases
- **Context:** Real top-ups convince judges; mainnet ZEC adds cost and 75 s blocks.
- **Decision:** testnet for ZEC; real Bitrefill, protected by invite allowlist + product allowlist + ₹200/day global limit (review R-1).
- **Consequences:** + 25 s blocks, free coins. − Free coins buy real goods, so access control is mandatory (FR-1, BR-1..3).

## ADR-009 Top-ups only for v1
- **Context:** Gift cards and eSIMs are resellable codes (higher abuse value) and widen scope (review R-1, R-9).
- **Decision:** Jio/Airtel/Vi top-ups from ₹10 to ₹50 only.
- **Consequences:** + low abuse value, India-relevant, cheap demos. − Narrower story; eSIM is on the roadmap ("journalist emergency eSIM").

## ADR-010 Long polling, single laptop
- **Context:** No public URL at the venue.
- **Decision:** teloxide long polling; all state on the laptop.
- **Consequences:** + no infra. − Single point of failure → recorded backup video.

## ADR-011 Static landing page + outbound tunnel for live stats
- **Context:** Judges need a page that stays up when the laptop is off (FR-15). Live stats (FR-16) live in the laptop's SQLite. NFR-10 allows no inbound ports.
- **Decision:** plain HTML/CSS/JS in `web/`, uploaded as Cloudflare Workers static assets with `npx wrangler deploy` (wrangler 4.156 delegates new Pages projects to Workers; direct upload: no git remote, so no commit history is published; no build step, no framework). The `ssa` binary serves `GET /stats.json` on `127.0.0.1` (axum, same tokio runtime). During the demo a `cloudflared` quick tunnel (outbound only, no account) publishes it; its URL goes in `web/config.js` and the page is redeployed.
- **Alternatives:** serve the page from the laptop (down whenever the laptop is); push stats to the static host every minute (commit spam); a server-side receipt viewer (needs users' UFVKs, which breaks the privacy boundary).
- **Consequences:** + page always up, bot unaffected if the tunnel dies. − The quick-tunnel URL changes each run (redeploy `config.js`, ~1 min). − One more crate (axum). − trycloudflare has no uptime promise, so the page degrades to "offline".
