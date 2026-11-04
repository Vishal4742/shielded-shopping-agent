# 10 — Risk Register

Scale: Likelihood (L) and Impact (I) 1–5; Score = L×I. Review at each phase boundary on hack day.

| ID | Risk | L | I | Score | Mitigation | Contingency | Owner |
|---|---|---|---|---|---|---|---|
| RK-01 | zcash-devtool CLI flags/output differ from the design | 4 | 3 | 12 | P0.5 transcript + fixtures; pin commit | Adjust parser; zingo-cli fallback | A |
| RK-02 | Hosts disallow any pre-event work beyond docs | 2 | 4 | 8 | Ask early (P0.1); spikes as transcripts only | Redo setup on the day (≤ 30 min) | User |
| RK-03 | testnet.zec.rocks down on hack/demo day | 2 | 4 | 8 | Ledger decouples purchases from the chain | Recorded video; mainnet fallback with tiny amounts | A |
| RK-04 | Bitrefill API rejects `auto_pay` with a personal key | 2 | 5 | 10 | P0.6 manual + API test before the event | Ask Bitrefill mentors; dry-run demo + manual purchase | B |
| RK-05 | Double purchase / lost money via crash | 2 | 4 | 8 | FSM + conditional updates + UNCERTAIN + no auto-retry | Operator checks Bitrefill dashboard | B |
| RK-06 | Abuse drains the Bitrefill balance | 2 | 4 | 8 | Allowlist, product allowlist, ₹200/day, kill switch | `SSA_PAUSED=1`; revoke API key | Operator |
| RK-07 | LLM latency or cost too high (low API balance) | 3 | 2 | 6 | Haiku 5.5, low effort, one call, max_tokens 256 (Q6 closed); tests use fixtures | Regex-only | B |
| RK-08 | Rust compile/debug time eats the schedule | 3 | 4 | 12 | Few crates; versions checked (P0.9); pure modules TDD | Cut list §5 | Both |
| RK-09 | Venue Wi-Fi unreliable | 3 | 4 | 12 | Phone hotspot | Recorded video | Both |
| RK-10 | Judge's number not on a supported operator | 3 | 2 | 6 | Support Jio, Airtel, Vi | Team member's SIM | Both |
| RK-11 | Judges see "Bitrefill agent" as unoriginal (402Cards) | 3 | 3 | 9 | Lead with shielded budget + viewing-key receipts | Pitch the journalist-eSIM roadmap | Both |
| RK-12 | Receipt sends stuck (notes not spendable) | 3 | 2 | 6 | Pre-fund early; ledger decouples; retry queue | Show a receipt sent earlier | A |
| RK-13 | Faucet coins insufficient | 3 | 3 | 9 | Collect TAZ daily from 2 faucets starting 30 Oct | Ask Zcash mentors for TAZ | User |
| RK-14 | Anthropic API headers/params wrong | 2 | 3 | 6 | P0.7 real curl fixture | Regex fallback | B |
| RK-15 | Laptop loss/failure | 1 | 5 | 5 | Push the repo often; FileVault | Second laptop with the repo + `.env` backup on encrypted USB | Both |
| RK-16 | Tunnel URL changes each run or trycloudflare is down at the demo | 3 | 1 | 3 | Start the tunnel once with `--protocol http2`, update `web/config.js`, `npx wrangler deploy` (rehearsed in P0.12) | Page shows "offline"; show `/stats.json` on the laptop | B |

**Top risks:** RK-01, RK-08, RK-09 (score 12). All have mitigations in Phase 0 or the cut list.
