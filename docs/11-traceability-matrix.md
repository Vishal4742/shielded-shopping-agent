# 11 — Requirements Traceability Matrix

Requirement → design element → test cases. Every M requirement has at least one test.

| Req | Pri | Design (05 / 04) | ADR | Threat | Tests |
|---|---|---|---|---|---|
| FR-1 Access | M | bot allowlist; `invites` table | 008 | T1, T15 | TC-F01, F02 |
| FR-2 Private only | M | bot private guard | — | T3, T8 | TC-F03 |
| FR-3 Budget wallet | M | zec init; zip321; §5.1 | 002, 003 | — | TC-F04 |
| FR-4 Deposit ledger | M | sync worker §3.5; ledger UNIQUE(txid) | 005 | — | TC-F05 |
| FR-5 Balance | M | ledger available §1.1 | 005 | — | TC-F06 |
| FR-6 Intent | M | intent §4; regex fallback | 006, 007 | T2, T13 | TC-F07, F08, TC-S1 |
| FR-7 Quote | M | quote §3.1 | — | — | TC-F09 |
| FR-8 Confirm and limits | M | confirm transaction §3.2; caps §3.6 | — | T3, T4 | TC-F10, F11, TC-B01..B05, TC-R1, TC-S2 |
| FR-9 Purchase | M | purchase §3.3; Bitrefill §5.2 | 004 | T11 | TC-F12, F13, TC-F2 |
| FR-10 Receipt | M | receipt queue §3.4 | 005 | T6, T14 | TC-F14, TC-R3 |
| FR-11 Viewing key | M | `/viewkey`; one account (UFVK) per user | 003 | T8 | TC-F15 |
| FR-12 Gift | C | — (stretch) | — | — | — |
| FR-13 Kill switch | M | `SSA_PAUSED` read at ✅ | — | T1 | TC-F16 |
| FR-14 Uncertain | M | FSM UNCERTAIN; recovery §6 | — | T5 | TC-R2 |
| FR-15 Landing page | S | `web/` static page §10.1–10.2, 10.5–10.6 | 011 | — | TC-W1 |
| FR-16 Live stats | C | `web` module `/stats.json` §10.3–10.4 | 011 | T16 | TC-W2, W3, W4, TC-S5 |
| BR-1 Products | M | caps | 009 | T1 | TC-B01 |
| BR-2 User/day | M | caps (IST) | — | T1 | TC-B02, B05 |
| BR-3 Global/day | M | caps | 008 | T1 | TC-B03 |
| BR-4 Rate | M | caps | — | T1 | TC-B04 |
| BR-5 Counted states | M | caps spend query | — | — | TC-B02, B03 |
| BR-6 Expiry | M | quote_expires_at | — | — | TC-F10 |
| NFR-1 ✅ ≤ 15 s | M | ledger, no chain on critical path | 004, 005 | — | TC-P1 |
| NFR-2 Reply ≤ 5 s | M | one LLM call, low effort | 006 | — | TC-P2 |
| NFR-3 No leaks | M | log masking; gitignore | — | T7, T9 | TC-S3, S4 |
| NFR-4 LLM can't buy | M | no buy tool | 006 | T2 | TC-S1 |
| NFR-5 No doubles | M | FSM, conditional updates, memo search | — | T4, T5, T6 | TC-R1..R3 |
| NFR-6 Serialised wallet access | M | wallet mutex | 002 | — | TC-R4 |
| NFR-7 Clear messages | M | catalogue §9 | — | — | Review |
| NFR-8 Dependency failures | M | error handling | — | T11, T12 | TC-F1..F3 |
| NFR-9 Unit tests | M | pure modules | — | — | `cargo test` |
| NFR-10 Laptop only | M | long polling; stats via outbound tunnel only | 010, 011 | T16 | Demo setup, TC-W4 |
