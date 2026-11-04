# 12 — Operations Runbook

For the operator on hack day and demo day. Commands get filled in once the code exists.

## 1. Prerequisites
- `.env` filled in (05 §8); `data/` exists; FileVault on
- zcash-devtool built at the pinned commit; path set in `SSA_DEVTOOL`
- Treasury wallet synced; treasury UA in `.env`
- Bitrefill balance ≥ $10; Anthropic credit ≥ $5
- Phone hotspot charged and tested

## 2. Start / stop
| Action | How |
|---|---|
| Start | `cargo run --release` (or the built binary) from the repo root |
| Stop | Ctrl-C; safe at any time (recovery runs on the next boot) |
| Pause purchases | Set `SSA_PAUSED=1` in `.env` (read at every ✅); no restart needed |
| Resume | `SSA_PAUSED=0` |

## 3. Demo-day prep (night of 4 Nov)
1. Create 5 judge invite codes; print them on the slide.
2. Pre-create and pre-fund 3–5 demo wallets (≥ 0.05 TAZ each) so judges can buy without waiting for confirmations.
3. Run TC-F12 once (real ₹10) to confirm Bitrefill is live.
4. Record the backup video of the full flow.
5. Check the global limit: is today's usage low enough to do the demo?

## 4. Incident playbook
| Symptom | Likely cause | Action |
|---|---|---|
| "Can't reach the Zcash network" | lightwalletd down | Purchases still work from the ledger; tell judges receipts will follow; check https://testnet.zec.rocks |
| Order stuck UNCERTAIN | Crash/timeout after invoice | Check the Bitrefill dashboard → mark DELIVERED or FAILED (+ REVERSAL) by hand; tell the user |
| Many unknown users hitting the bot | Bot handle leaked | They're refused by the allowlist; if costs rise, rotate the bot token |
| Bitrefill balance low | Many demos | Top up on the website; or pause |
| LLM errors | Key/credit/outage | Regex fallback is automatic; tell users the strict format |
| Wi-Fi drops | Venue | Switch to the hotspot; restart the bot (safe) |
| Everything down | — | Play the backup video |

## 5. After the event
- Rotate the Telegram, Bitrefill and Anthropic keys.
- Export notes and defects; wipe `data/` or move it to encrypted storage.
- Start the 30-day milestone (09 §8).
