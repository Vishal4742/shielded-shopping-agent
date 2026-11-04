# 09 — Project Plan (WBS and schedule)

v1.0 · 30 Oct 2026 · Supersedes draft `03-plan.md`

## 1. Lifecycle model
Plan-driven before the event (requirements → design → review → prep); time-boxed incremental
build on hack day, with a working, committed increment at every phase gate.

**Rule (C3):** no project source code before 4 Nov 11:00 unless the hosts approve in writing.
Prep spikes are **shell transcripts only** (no Rust), and all prep is listed in the README (review R-13).

## 2. Work breakdown structure

```
1 Management          1.1 Charter ✔  1.2 Plan ✔  1.3 Risk register ✔  1.4 Host Q&A (Q1, Q4)
2 Analysis            2.1 Feasibility ✔  2.2 SRS ✔  2.3 Threat model ✔
3 Design              3.1 Architecture ✔  3.2 Detailed design ✔  3.3 ADRs ✔  3.4 Design review ✔ (fixes applied)
4 Prep (Phase 0)      4.1 Accounts/keys  4.2 Tool builds  4.3 Spikes + fixtures  4.4 Demo assets
5 Build (hack day)    5.1 Skeleton  5.2 Wallet  5.3 Orders+Bitrefill  5.4 Intent  5.5 Receipts  5.6 Harden  5.7 Web page
6 Verification        6.1 Unit  6.2 Integration  6.3 System/acceptance  6.4 Security  6.5 Performance
7 Release             7.1 README  7.2 Video  7.3 Slides  7.4 Submission  7.5 Bitrefill feedback
8 Post-event          8.1 30-day milestone  8.2 Key rotation, cleanup
```

## 3. Phase 0: prep (30 Oct – 3 Nov)

| ID | Task | Output | Owner | Depends |
|---|---|---|---|---|
| P0.1 | Ask hosts: pre-built code? WebZjs status? | Answers in `notes/host-qa.md` | User | — |
| P0.2 | Install the three-techniques harness (rust pack) | `.claude/`, `CLAUDE.md` | User runs | P0.1 (if code-like config is disallowed, keep it outside the repo) |
| P0.3 | Build zcash-devtool (pin commit `0b3436b`) | Binary path for `.env` | User runs | — |
| P0.4 | Testnet wallets: treasury + 1 test; **collect TAZ every day** | ≥ 3 TAZ | Both | P0.3 |
| P0.5 | Devtool transcript: init, address, sync, balance, send with memo, list-tx csv, UFVK export/import, multiple accounts (Q2), min-confirmations flag | `notes/devtool.md` + raw output fixtures | Claude | P0.4 |
| P0.6 | Bitrefill: account, key, $20 balance, manual ₹10 Jio; GET product JSON for 3 operators; check the invoice reference field (Q5) | `notes/bitrefill.md` + JSON fixtures | User + Claude | — |
| P0.7 | Anthropic: one real curl with the planned body (model, effort, format, fallbacks) | `notes/anthropic.md` + response fixture | Claude | key |
| P0.8 | Telegram bot token; bot name, description, privacy mode | Token in `.env` | User | — |
| P0.9 | Crate version check: write down candidate versions (teloxide, reqwest, rusqlite, qrcode, tokio) from crates.io; no Rust code | `notes/stack.md` | Claude | — |
| P0.10 | Zingo on phone (testnet); 3–5 judge wallets pre-funded the night before | Ready | User | P0.4 |
| P0.11 | Demo script, 3 slides, invite codes | `docs/demo.md`, slides | Both | — |
| P0.12 | Web spike (shell only, throwaway hello page outside the repo): `brew install cloudflared qrencode`; `cloudflared tunnel --protocol http2 --url` to `python3 -m http.server`, curl it from a phone; Cloudflare account + `npx wrangler deploy` of the hello page as Workers static assets (no git remote); `qrencode -t SVG` for the bot link; add the axum version to P0.9 | `notes/web.md`, bot QR SVG | Claude + User | — |

**Phase 0 exit:** every external dependency has been exercised once, with output saved.

## 4. Hack-day schedule (4 Nov, 11:30–23:45)

| Phase | Time | WBS | Deliverable | Gate (build + test + manual) |
|---|---|---|---|---|
| 1 Skeleton | 11:30–12:30 | 5.1 | Crate, config, DB schema + migrations, `/start` with allowlist + private guard | TC-F01..F03 |
| 2 Wallet | 12:30–14:30 | 5.2 | `zec` module with locks, parsers (fixture tests), wallet per user, QR, sync worker, ledger credits | TC-F04..F06 |
| 3 Orders | 14:30–16:15 | 5.3 | `caps`, `quote`, `order_fsm` (TDD), Bitrefill client, confirm transaction, purchase + polling, recovery | TC-F09..F13, TC-B*, TC-R1 |
| 4 Intent | 16:15–17:30 | 5.4 | Claude extraction + regex fallback, quote card | TC-F07, F08, TC-S1 |
| **⏱ 18:00** | | | **Go/no-go: TC-F12 (real ₹10) passes end to end** | Else apply the cut list |
| 5 Receipts + web | 18:00–19:45 | 5.5, 5.7 | A: receipt queue, memo search, `/viewkey`. B: landing page `web/` (FR-15, ~100 min incl. ~45 min of assets, 05 §10.6) + `/stats.json` route (FR-16, ~30 min). Solo: FR-15 after receipts; FR-16 only if receipts are green by 19:00 | TC-F14, F15, TC-R3, TC-W1..W4 |
| 6 Harden | 19:45–21:30 | 5.6, 6.* | Recovery tests, failure messages, log masking, clippy, perf runs | TC-R2, R4, TC-F1..F3, TC-S*, TC-P* |
| 7 Release | 21:30–23:45 | 7.* | README (incl. prep disclosure), video, slides, submission, feedback notes | Submitted |

## 5. Cut list (in order)
1. FR-12 gift (already stretch)
2. FR-16 live stats → page without the stats panel
3. FR-15 extras: Blender scan section, then decode card, then word reveal; last resort: a README section with the bot link and QR
4. Async receipts → receipts sent by hand during the demo by the operator via devtool
5. LLM → regex-only commands (`jio 98xxxxxx10 10`)
6. Per-user limits → keep global + product allowlist

**Never cut:** ✅ confirmation, allowlist, global limit, kill switch, secret hygiene, private-chat guard.

## 6. Roles (2-person team; solo = both)

| Role | Phases |
|---|---|
| A: Zcash/backend | 2, 5, recovery |
| B: Bot/Bitrefill/LLM | 1, 3 (Bitrefill), 4, release |
| Both | 18:00 checkpoint, Phase 6, demo |

## 7. Communication and tracking
Single source of truth: these docs plus `notes/`. A commit at every green gate, using conventional
commits. A quick standup at each phase boundary: what's green, what's blocked, whether to cut.

## 8. Post-event (5 Nov – 5 Dec)
30-day milestone: ZEC→USDC auto-refill via NEAR Intents into the Bitrefill balance; mainnet pilot;
eSIM product. Rotate all keys after demo day; wipe `data/` on the laptop.
