# 01 — Project Charter

| Field | Value |
|---|---|
| Project | Shielded Shopping Agent (working name "ZEC Concierge") |
| Event | Free Society Hackathon: WebZero × Zcash × Bitrefill, Mumbai |
| Build window | Wed 4 Nov 2026, 11:30 – 00:00 IST (12.5 h); demo Thu 5 Nov 12:00 |
| Sponsor / owner | madmax.co.in31@gmail.com (team lead) |
| Document status | Draft v0.2 · 30 Oct 2026 |

## 1. Vision

Pay for everyday digital goods (mobile recharge, eSIM, gift cards) with money that leaves no
public trail, by chatting with an AI agent in Telegram.

## 2. Business case

- **Problem:** every mobile recharge or gift card bought through UPI, a card or an exchange ties a real identity to the purchase. Public blockchains don't fix this, because anyone can read the ledger.
- **Opportunity:** shielded Zcash and Bitrefill's catalogue (Jio/Airtel/Vi from ₹10, India eSIMs) make private everyday purchases possible today. Nobody has packaged this as an AI agent. Prior Bitrefill agents (402Cards, the SingIt x402 agent) pay with public stablecoins.
- **Prize fit:** one project qualifies for the Privacy track (Zcash, $2,300 pool), the Agentic Commerce track (Bitrefill, $2,500 pool), and the 30-day ecosystem bonus. The organisers weight multi-track entries up.

## 3. Objectives (SMART)

| # | Objective | Measure | Target |
|---|---|---|---|
| O1 | A working end-to-end private purchase | Live ₹10 Jio top-up from a Telegram chat, paid from a shielded budget | Done by 18:00 on 4 Nov |
| O2 | Show what privacy adds | Purchase receipts visible only through the user's viewing key | Shown live on demo day |
| O3 | Safe by construction | No purchase without ✅; spend caps enforced in code | 100% of acceptance tests pass |
| O4 | Submit on time | Repo, README, video, and submission form | Before 23:45 on 4 Nov |
| O5 | Place in both tracks | Shortlisted for demo day | Top 6 |

## 4. Scope

**In:** Telegram bot; per-user shielded testnet budget; natural-language ordering; Bitrefill purchase from a prefunded balance; spend caps; shielded receipt memos; viewing-key export; gift-to-handle (should-have).

**Out:** non-custodial key management; ZEC→USDC auto-refill; mainnet; web or mobile app; standing rules; group pools. Details in the SRS §2.6.

## 5. Stakeholders

| Stakeholder | Interest | Engagement |
|---|---|---|
| Team (1–2 builders) | Ship and win | Owns all documents |
| Hackathon judges (technical and non-technical) | Working demo, real Zcash use, originality | Demo day |
| Zcash ecosystem (ECC, ZF, ZecHub) | Real shielded usage, ecosystem growth | Mentors on 4 Nov |
| Bitrefill | Agents that actually buy; API feedback | Mentors; feedback prize |
| WebZero (organiser) | Briefs fulfilled, 30-day follow-through | Rules, judging |
| End users (buyers, gift recipients, auditors) | Privacy, ease, trust | Personas in the SRS |

## 6. Constraints

- 12.5 h build time, in person, with no project source code written before the event (pending the hosts' answer)
- Testnet ZEC only; real Bitrefill balance capped at about $20
- Zcash tooling has been in flux since the Ironwood upgrade (Jul 2026): WebZjs and the MetaMask Snap are unusable
- Stack: Rust wherever possible (team decision)

## 7. Assumptions

- `zcash-devtool` builds on macOS and works against `testnet.zec.rocks:443`
- The Bitrefill personal API key supports `auto_pay` from the balance for Indian top-ups
- Venue Wi-Fi works, with a phone hotspot as backup

## 8. High-level milestones

| Milestone | Date |
|---|---|
| Documents approved (this set) | 31 Oct |
| Prep complete: accounts, tool builds, spikes (plan Phase 0) | 3 Nov |
| MVP end to end | 4 Nov 18:00 |
| Submission | 4 Nov 23:45 |
| Demo | 5 Nov 12:00 |
| 30-day milestone delivered | 5 Dec |

## 9. Budget

| Item | Cost |
|---|---|
| Bitrefill prefunded balance | ~$20 |
| Claude API usage (testing and demo) | ~$5–10 |
| Testnet ZEC | Free (faucets) |
| Hosting | None (laptop) |

## 10. Approval

| Role | Name | Decision | Date |
|---|---|---|---|
| Team lead | | ☐ Approved | |
