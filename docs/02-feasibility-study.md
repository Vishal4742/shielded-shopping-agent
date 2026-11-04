# 02 — Feasibility Study

Status: v1.0 · 30 Oct 2026 · Based on six research threads plus live API tests (sources §8)

## 1. Purpose

Decide which project to build for the Free Society Hackathon, and whether it can be built in
12.5 hours with the tools available in late October 2026.

## 2. Options considered

Ideas came from past winners at Zcash, Ethereum, Solana, Bitcoin/Lightning, TON, Celo, Algorand
and other privacy-chain hackathons, adapted to Zcash and Bitrefill. Ideas already built and awarded
on Zcash were excluded: UPI POS (Zapp), TypeScript SDKs (t2z, Temi), memo-OTP login (ZcashMe),
a memo-triggered bot (Zeke), poker, and a FROST wallet (CYZE).

| # | Option | Inspired by |
|---|---|---|
| A | **Shielded shopping agent (Telegram)** | SingIt, Superpage, Sp3nd, Almoner |
| B | Claim-link payouts to a Telegram handle | mailMIST / MIST.cash, Settlr Pay |
| C | Burner Line: SOS eSIM for reporters | HRF AI Hack for Freedom winners |
| D | Shielded dead-man switch | canary (W3PN), Legacy Vault (Zama) |
| E | Open Books: NGO audit through a viewing key | zkpoor (btc++) |

## 3. Technical feasibility

| Capability needed | Finding | Risk |
|---|---|---|
| Programmatic shielded wallet | `zcash-devtool` (ECC, updated Oct 2026): init, address, sync, balance, send with memo, UFVK export/import | Low–Med (dev-grade CLI) |
| Browser wallet (WebZjs / MetaMask Snap) | Last updated Apr 2026, before Ironwood; open breakage reports | **High: excluded** |
| Zallet RPC wallet | Needs a local Zebra node; Linux only; no NU7 release yet | **High: excluded** |
| Network access | Testnet: only `testnet.zec.rocks:443` is live; mainnet has several servers | Med (single point of failure) |
| Block time | Testnet ~25 s (NU7 active); mainnet 75 s | Low on testnet |
| Bitrefill API | Self-serve personal key; `POST /invoices` with `auto_pay` from balance | Low |
| India catalogue | Jio, Airtel and Vi from ₹10; India eSIM from $3.89; Amazon.in out of stock | Low |
| Paying Bitrefill in ZEC | Not accepted directly. NEAR 1Click quotes ZEC→USDC (Base) in ~7.6 min | High live → **use prefunded balance** |
| Telegram bot in Rust | teloxide, mature | Low |
| Claude in Rust | No official SDK; raw HTTP to the Messages API | Low |

**Verdict:** feasible, using devtool + Bitrefill balance + Rust. Excluding WebZjs/Snap goes
against one organiser brief, so ask at the workshop whether a fixed build exists.

## 4. Economic feasibility

| Cost | Amount | Return |
|---|---|---|
| Bitrefill float | ~$20 | Needed for live demos |
| Claude API | ~$5–10 | — |
| Team time | 12.5 h build + ~6 h prep | Prize pools: Privacy $2,300 + Agentic $2,500 + bonus |

Positive expected value: low cost, and two prize pools are reachable with one build.

## 5. Operational feasibility

Users already live in Telegram. A judge can try it on their own phone in under 60 seconds. Recharges
in India start at ₹10, so a live demo costs about $0.11. The main operational risk is venue
connectivity; the mitigations are a hotspot and a recorded video.

## 6. Schedule feasibility

| Option | Est. build h | Fits 12.5 h? |
|---|---|---|
| A | 9–10 (+1.5 per extra feature) | ✅ with a cut list |
| B | 7–8 | ✅ |
| C | same as A | ✅ (alternative pitch on the same code) |
| D | 8 | ✅ but demo needs a time skip |
| E | 6–7 | ✅ |

## 7. Legal, policy and rules feasibility

- **Hackathon rules:** no rule on pre-built code is published. Default: write all source on the day; docs and setup before. Ask the hosts (open question Q1).
- **Custody:** the bot holds testnet keys for its users. Acceptable for a testnet demo; say so in the pitch; non-custodial design is on the roadmap.
- **Bitrefill terms:** personal API use for your own purchases is allowed. Reselling to the public on mainnet would need Bitrefill's business API (out of scope).
- **KYC/AML:** testnet-only, with tiny real-value purchases from our own balance, so no regulated activity during the event.

## 8. Option scoring (1–5; weights reflect what judges reward)

| Criterion (weight) | A | B | C | D | E |
|---|---|---|---|---|---|
| Uses Zcash-only features (0.20) | 4 | 4 | 4 | 4 | 4 |
| Covers both tracks (0.20) | 5 | 4 | 5 | 1 | 1 |
| Originality vs prior winners (0.15) | 3 | 4 | 4 | 4 | 2 |
| Live-demo reliability (0.20) | 5 | 3 | 4 | 2 | 3 |
| Build risk in 12.5 h (0.15) | 3 | 3 | 3 | 3 | 4 |
| Theme fit "free society" (0.10) | 4 | 3 | 5 | 5 | 3 |
| **Weighted total** | **4.10** | 3.55 | **4.15** | 2.95 | 2.80 |

A and C share one codebase, so the decision is: **build A, pitch with C's story as the closing
line.** B's claim link becomes the gift feature of A.

## 9. Recommendation

Proceed with Option A (Shielded Shopping Agent) using Rust, zcash-devtool, Bitrefill prefunded
balance, Telegram and Claude. Go/no-go checkpoint: the MVP must work end to end by 18:00 on hack
day, otherwise follow the cut list (project plan §5).

## 10. Sources

Zypherpunk winners (forum.zcashcommunity.com/t/53985) · ZecHub Hackathon 3.0 winners
(zechub.substack.com) · Hack-to-Dark grant (github.com/ZcashCommunityGrants/…/issues/398) ·
Bitrefill docs (docs.bitrefill.com) and live catalogue data · NEAR 1Click `/v0/tokens` and dry
quote (live, 30 Oct) · zcash-devtool repo (github.com/zcash/zcash-devtool, commit 0b3436b) ·
ETHGlobal, Colosseum, btc++, HRF, Celo, AlphaTON and Algorand winner pages (see the research notes
from the chat of 30 Oct).
