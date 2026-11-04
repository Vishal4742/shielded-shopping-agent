# 03 — Software Requirements Specification (SRS)

Structure follows ISO/IEC/IEEE 29148 · v1.1 · 30 Oct 2026 (v1.1 adds the web page: EI-6, FR-15, FR-16, NFR-10 reworded) · Supersedes the draft `01-requirements.md`
Incorporates the staff-engineer review of 30 Oct (findings R-1 … R-15).

## 1. Introduction

### 1.1 Purpose
This document states what the Shielded Shopping Agent (SSA) must do and how well. It is the
reference for design (04, 05), tests (08) and traceability (11).

### 1.2 Scope
SSA is a Telegram bot. Allowlisted users chat with it to buy Indian mobile top-ups, paid from a
personal shielded Zcash (testnet) budget. Purchases go through Bitrefill from a prefunded balance.
Each purchase leaves an encrypted receipt that only the user's viewing key can read.
A public landing page explains the product and shows live aggregate stats; buying happens only in Telegram.

### 1.3 Definitions

| Term | Meaning |
|---|---|
| ZEC / TAZ | Zcash mainnet coin / testnet coin |
| Shielded | Funds and memos encrypted on-chain (Orchard/Ironwood pool) |
| UA | Unified Address: a Zcash address that can contain shielded receivers |
| UFVK | Unified Full Viewing Key: read-only key that shows a wallet's history including memos |
| Memo | Encrypted 512-byte note attached to a shielded transfer |
| ZIP-321 | Standard `zcash:` payment URI format |
| ZIP-317 | Zcash fee rule (5,000 zat per logical action, minimum 2 actions = 10,000 zat) |
| zat | 1e-8 ZEC |
| Bitrefill | Merchant selling top-ups, gift cards and eSIMs via API |
| Budget | A user's ZEC balance held in their bot-managed shielded wallet |
| Receipt | Shielded transfer, user wallet → treasury, whose memo describes one order |
| IST | Indian Standard Time; all "per day" limits reset at 00:00 IST |

### 1.4 References
01 Project Charter · 02 Feasibility Study · Bitrefill API v2 docs · ZIP-317, ZIP-321 · zcash-devtool README · Telegram Bot API · Anthropic Messages API

## 2. Overall description

### 2.1 Product perspective
A new, standalone system that combines four external services: the Telegram Bot API, the Zcash
testnet (through the zcash-devtool CLI and lightwalletd), the Bitrefill API, and the Anthropic
Messages API. It runs as one process on a laptop. See 04 Architecture.

### 2.2 Product functions (summary)
1. Admit allowlisted users through an invite code
2. Give each user a shielded budget wallet and deposit QR
3. Understand "Recharge Jio 98xxxxxx10 ₹10" in plain language
4. Quote, ask for confirmation, enforce limits, and buy the top-up
5. Write a shielded receipt per purchase; export the viewing key
6. Explain itself on a public landing page with live aggregate stats (FR-15, FR-16)

### 2.3 User classes

| Class | Description | Frequency | Privilege |
|---|---|---|---|
| Buyer | Team member or judge with an invite | Demo-day bursts | Buy within caps |
| Auditor | Anyone given a buyer's UFVK | Rare | Read-only, outside the bot |
| Operator | Team member running the laptop | Always | `.env`, allowlist, kill switch |

### 2.4 Operating environment
macOS laptop · Rust stable (1.99) · zcash-devtool binary · Zcash **testnet** via `testnet.zec.rocks:443` · Internet (venue Wi-Fi or a phone hotspot) · Telegram clients on users' phones.

### 2.5 Design and implementation constraints
- C1 Rust for all project code (team decision)
- C2 No WebZjs, MetaMask Snap or Zallet (broken or unavailable after Ironwood; see 02 §3)
- C3 Project source written only during the event window, pending the hosts' answer (Q1)
- C4 Real Bitrefill balance ≤ US$20; real spend ≤ ₹200 per IST day
- C5 Secrets only in `.env` and `data/`, both gitignored

### 2.6 Out of scope (this release)
eSIMs, gift cards and other products · gifts to handles (stretch only, FR-12) · non-custodial keys · ZEC→USDC auto-refill · mainnet · standing rules · group pools · a web or mobile app for buying or viewing receipts (the landing page is informational only; a server-side receipt viewer would need users' UFVKs) · group-chat use.

### 2.7 Assumptions and dependencies

| # | Assumption | If false |
|---|---|---|
| A1 | zcash-devtool builds on macOS and syncs testnet | Fall back to zingo-cli (JSON output) |
| A2 | Bitrefill `auto_pay` from balance works with a personal key for Indian top-ups | Use the website by hand for the demo; show the API flow on dry-run |
| A3 | `testnet.zec.rocks` stays up | Demo from recorded video; mainnet fallback with tiny amounts |
| A4 | Judges will use a pre-funded demo wallet | — |

## 3. Specific requirements

Priority: **M** must · **S** should · **C** could (MoSCoW).

### 3.1 External interface requirements

| ID | Interface | Requirement |
|---|---|---|
| EI-1 | Telegram (user) | Private chats only. Commands: `/start [invite]`, `/balance`, `/viewkey`, `/help`; free text for orders; inline ✅/❌ buttons |
| EI-2 | zcash-devtool (software) | Invoked as a subprocess. Every call on a wallet holds that wallet's lock. Output parsed from text or CSV |
| EI-3 | Bitrefill API v2 (software) | Bearer auth. Product lookup, invoice create with `payment_method=balance`, `auto_pay=true`, invoice/order polling |
| EI-4 | Anthropic Messages API (software) | One structured-output call per user message to extract an order intent |
| EI-5 | lightwalletd gRPC (communications) | Through devtool only; TLS to `testnet.zec.rocks:443` |
| EI-6 | Web (public) | Static landing page on free static hosting. It reads `GET /stats.json` (aggregates only) from the bot through an outbound tunnel |

### 3.2 Functional requirements

| ID | Pri | Requirement | Acceptance |
|---|---|---|---|
| **FR-1 Access** | M | The bot serves only Telegram user ids on the allowlist. `/start <invite>` with a valid invite code adds the user. Anyone else gets a polite refusal and no wallet | Unknown user → refused; valid invite → admitted |
| **FR-2 Private only** | M | The bot ignores every command and button in group chats, apart from a "DM me" hint | Group message → no action |
| **FR-3 Budget wallet** | M | On first admission the bot creates a dedicated shielded testnet account (own ZIP-32 account and UFVK) for the user and shows its UA as text, a ZIP-321 URI and a QR | New user sees an address within 10 s |
| **FR-4 Deposit ledger** | M | The bot records confirmed incoming shielded transfers to a user's wallet as ledger credits (once per txid) | Deposit appears in `/balance` after confirmation |
| **FR-5 Balance** | M | `/balance` shows the available budget = credits − debits (ZEC and ≈₹), plus "pending deposits" | Matches ledger |
| **FR-6 Intent parsing** | M | Free text is turned into `{operator ∈ Jio/Airtel/Vi, phone, amount_inr}` or a clarifying question. Phone and amount are re-validated in code | "recharge jio 98xxxxxx10 ten rupees" → correct intent |
| **FR-7 Quote** | M | The bot creates a quote with Bitrefill's USD price converted to zat (rounded up) + the ZIP-317 fee, valid for 10 min, and shows ✅/❌ | Quote card shows ₹, ZEC, fee, expiry |
| **FR-8 Confirm and limits** | M | On ✅, code atomically checks: caller owns the quote; status is QUOTED; not expired; product allowlisted; caps (BR-1..4); available ≥ price + fee. Then it moves the order to PURCHASING | Any failed check → clear message, no purchase |
| **FR-9 Purchase** | M | The bot creates a Bitrefill invoice (balance, auto_pay), sending the order id as a reference if the API allows. It polls to completion and sends the result only to the buyer's private chat | Top-up arrives on the phone |
| **FR-10 Receipt** | M | After DELIVERED, a receipt queue sends price + fee from the user's account to the treasury UA with memo `SSA·<order>·<operator>·₹<amt>`. It retries until notes are spendable | Memo visible via UFVK |
| **FR-11 Viewing key** | M | `/viewkey` returns the user's UFVK and how to import it, in the private chat only | Second wallet imports it and sees memos |
| **FR-12 Gift to handle** | C | `/gift @user ₹N` creates a claim link usable only by that username | Stretch; not in the demo critical path |
| **FR-13 Kill switch** | M | An operator env flag `SSA_PAUSED=1` (read at each ✅) refuses all purchases | Flag set → purchases refused |
| **FR-14 Uncertain orders** | M | If the purchase result is unknown (crash or timeout after the invoice call), the order becomes UNCERTAIN and the user sees "checking with Bitrefill". It is never auto-retried | Kill test (08 TC-R2) |
| **FR-15 Landing page** | S | One static page: what SSA does and why it's private, how it works in 3 steps, an "Open in Telegram" button + QR to the bot, try-it steps for judges, the limits, and a repo link. No forms, cookies, trackers or third-party requests (fonts self-hosted). Readable on a phone | Phone opens the page; the button opens @zcashdemo_bot |
| **FR-16 Live stats** | C | The bot serves `GET /stats.json`: top-ups delivered today, ₹ committed today vs the ₹200 cap, total shielded receipts, the paused flag, and the update time. No per-user or per-order data. Cached ≤ 10 s. The page polls it; if it can't be reached, the page shows "Live stats offline" and everything else still works | Delivered order → page count +1 within 30 s; tunnel stopped → offline notice |

### 3.3 Business rules

| ID | Rule |
|---|---|
| BR-1 | Allowed products: Jio, Airtel and Vi top-ups only, ₹10–₹50 per order |
| BR-2 | Per-user daily limit ₹100 (IST day) |
| BR-3 | Global daily limit ₹200 (IST day), protecting the real Bitrefill balance |
| BR-4 | At most one purchase per user per 30 s |
| BR-5 | Cap usage counts orders in CONFIRMED, PURCHASING, UNCERTAIN, DELIVERED, RECEIPT_* states |
| BR-6 | Quotes expire after 10 min |

### 3.4 Use cases

**UC-1 Join and fund** · Actor: Buyer · Pre: holds an invite code
1. Buyer sends `/start <invite>` → FR-1 admits them → FR-3 creates the wallet and shows the QR.
2. Buyer pays TAZ from Zingo (or the operator pre-funds the wallet the night before; A4).
3. After confirmation, FR-4 credits the ledger, and `/balance` shows the amount.
*Alt:* invalid invite → refusal.

**UC-2 Buy a top-up** · Actor: Buyer · Pre: funded budget
1. Buyer: "Recharge Jio 98xxxxxx10 ₹10" → FR-6 → FR-7 quote card.
2. Buyer taps ✅ → FR-8 checks → FR-9 purchase → "Done ✅ Jio ₹10 to 98xxxx…10".
3. In the background: FR-10 receipt.
*Alt:* ❌ / expiry → cancelled · limit or budget fail → message · Bitrefill error → FAILED, "not charged" only when certain · unknown → UNCERTAIN.

**UC-3 Audit** · Actor: Auditor
1. Buyer sends `/viewkey` and gives the UFVK to the auditor.
2. Auditor imports it into a watch-only wallet (devtool/Zingo) and sees deposits plus receipt memos, and nothing from other users.

### 3.5 Non-functional requirements

| ID | Category | Requirement | Measure |
|---|---|---|---|
| NFR-1 | Performance | ✅ → top-up result ≤ 15 s (p90) when Bitrefill is healthy | Timed in TC-P1 |
| NFR-2 | Performance | Bot replies to a text within 5 s (p90), including the LLM call | Timed |
| NFR-3 | Security | No secret, redemption code or full phone number appears in logs or git | Secret grep, log review |
| NFR-4 | Security | The LLM can't cause a purchase; only FR-8 can | Design review + TC-S1 |
| NFR-5 | Reliability | No double purchase or double receipt across crash and restart | TC-R1..R3 |
| NFR-6 | Reliability | Concurrent devtool access to one wallet is serialised | Design + TC-R4 |
| NFR-7 | Usability | Every refusal tells the user why and what to do next | Review of message catalogue (05 §9) |
| NFR-8 | Availability | When a dependency is down, the bot says which one; it never drops an order silently | TC-F1..F3 |
| NFR-9 | Maintainability | Pure logic (limits, quote maths, state machine, parsers) has unit tests | `cargo test` |
| NFR-10 | Portability | Runs on one macOS laptop with no inbound ports (long polling). The bot works without any public URL; FR-16 is published only through an optional outbound tunnel | Demo setup; TC-W4 |

## 4. Verification
Each FR, BR and NFR maps to at least one test case in 08 Test Plan. The full map is in
11 Traceability Matrix.

## 5. Open questions

| # | Question | Default | Owner |
|---|---|---|---|
| Q1 | Is pre-event code allowed? | No: docs and setup only | Hosts |
| Q2 | Does devtool support multiple accounts per wallet? | **Answered 30 Oct: yes** → one wallet, one account per user (ADR-003) | Closed |
| Q3 | Will Bitrefill enable `-syldavia` test products? | Real ₹10 top-ups | Bitrefill |
| Q4 | Is a WebZjs fix available after Ironwood? | Not used | Workshop |
| Q5 | Does the Bitrefill invoice accept a client reference or idempotency key? | UNCERTAIN state (FR-14) | Phase 0 |
| Q6 | Claude model: `claude-opus-5-5` (low effort) or `claude-haiku-5-5` for speed? | **Closed 30 Oct: `claude-haiku-5-5`, low effort** (low API balance; P0.7 call: 497 in / 80 out tokens, correct JSON) | Team |
