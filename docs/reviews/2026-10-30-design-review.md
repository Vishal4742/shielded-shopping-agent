# Design Review — 30 Oct 2026

Reviewer: independent agent acting as a staff engineer (fresh context). Input: the draft requirements, design and plan.
Verdict: **ready with fixes**. All 15 findings were accepted and applied in baseline v1.0.

| # | Sev | Finding | Disposition |
|---|---|---|---|
| R-1 | Blocker | Free testnet coins can drain the real Bitrefill balance | FR-1 allowlist + invites; BR-1 top-ups ₹10–₹50 only; BR-3 ₹200/day; FR-13 kill switch; ADR-008/009 |
| R-2 | Blocker | Crash after a successful invoice → falsely "not charged" | FR-14 UNCERTAIN state; Q5 reference field; no auto-retry (05 §3.3) |
| R-3 | Blocker | Double tap → double purchase | `BEGIN IMMEDIATE` + conditional UPDATE (05 §3.2); TC-R1 |
| R-4 | Blocker | Concurrent devtool access to one wallet | Per-wallet mutex; sequential sync with skip-if-busy (04 §8, 05 §3.5); TC-R4 |
| R-5 | Major | Budget maths broken by change lock-up | Internal ledger decides spending (ADR-005, 05 §1.1) |
| R-6 | Major | Fee and FX rounding | Price from Bitrefill USD, ceil zat, + ZIP-317 fee (05 §3.1) |
| R-7 | Major | Duplicate receipts after a crash | RECEIPT_SENDING + memo search before resend (05 §3.4); TC-R3 |
| R-8 | Major | Judges can't deposit and buy in 60 s | Pre-funded judge wallets (runbook §3) |
| R-9 | Major | Scope too big (eSIM, gift, tool loop) | Top-ups only; single extraction call (ADR-006); gift → Could |
| R-10 | Major | Missing acceptance criteria | Added TC-F11, F13, B02–B05, F1–F3, S2; three operators |
| R-11 | Minor | Group-chat leaks | FR-2 private chats only |
| R-12 | Minor | Unverified Claude request shape | P0.7 real curl fixture |
| R-13 | Minor | Prep could look like pre-building | Transcripts only; wait for the hosts; disclose in the README |
| R-14 | Minor | Clippy every phase is too slow | Clippy in Phase 7 only |
| R-15 | Minor | "Today" and counted states undefined | IST days; BR-5 |
