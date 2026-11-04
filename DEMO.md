# Demo video script (3 minutes)

Judges watch only the first 3 minutes, and Bitrefill wants the real loop, not a mock. Record on a phone with a screen recording, Telegram open on [@zcashdemo_bot](https://t.me/zcashdemo_bot). Keep a laptop nearby for one optional beat (the kill switch).

**The timing problem.** An auto-pay rule never runs sooner than 2 minutes after you create it (`cmd_auto` in `src/telegram.rs` sets the first run to `now + max(period, 120 s)`), and the warning goes out 60 s before the run. So the real recording is about 2 minutes longer than the video. Record it in one take, then cut the two waits and put a caption on each cut ("+1 min"). That is the only editing. The script below is the edited timeline.

## Timeline (edited video)

| Time | Beat | Do this | On screen |
|---|---|---|---|
| 0:00–0:10 | **Problem** | Talk over the bot's chat, nothing typed. | Say: "An agent that can spend your money is scary. And paying with a normal wallet shows the world what you buy. This bot buys phone top-ups for you on a schedule. Code limits what it can spend, and each purchase leaves a receipt only you can read." |
| 0:10–0:25 | **Create the rule** | Type `/auto jio 9876543210 10 every 2m` and send. | A card: "🤖 New auto-pay rule", the rule line, "First run: HH:MM IST", the limits (₹10–₹50 per order, ₹100 per day), and two buttons: **✅ Allow auto-pay** and **❌ Cancel**. |
| 0:25–0:35 | **Approve once** | Tap **✅ Allow auto-pay**. | "✅ Auto-pay #1 is on … First run HH:MM IST. I'll ping you a minute before each run with a Skip button." Say: "This is the only tap. From here the agent works alone." |
| 0:35–0:50 | **The warning** | Cut the wait (caption "+1 min"). Don't tap anything. | "⏰ Auto-pay in about a minute: …" with **⏭ Skip this one** and **🛑 Stop rule**. Say: "I get a heads-up and can skip. If I do nothing, it runs." |
| 0:50–1:05 | **The run fires** | Cut the wait (caption "+1 min"). Don't tap. | "🤖 Auto-pay ran: …", then "✅ Top-up successful / Jio ₹10 → xxxxxxxx10 / HH:MM IST · Ref xxxxxxxx", then "Next run HH:MM IST". Say: "A real Bitrefill invoice, paid from our balance, polled until it was delivered. I never typed a buy." |
| 1:05–1:20 | **Shielded receipt** | Send `/viewkey`. | "Your viewing key (read-only): uviewtest1…" (blur it in the edit). Say: "Each purchase sends a shielded transaction with an encrypted memo, `SSA·<order>·jio·₹10`. Chain watchers see nothing. This key can read it, and can't spend." The receipt goes out within about 15 s of delivery and needs a block or two (25 s each) to be mined. |
| 1:20–1:55 | **/report: the viewing-key view** | Send `/report`. | "Building your report from your viewing key alone (a fresh watch-only wallet). This takes about 30 s…" then a file `ssa-receipts.csv` with the caption "Viewing-key report: N purchase receipt(s), ₹X in total. Built only from your read-only viewing key, the same view an auditor gets…". Speed up the 30 s wait to 2× or cut it, and caption it. Say: "A throwaway wallet, built from the key alone, found my receipts and nothing of the treasury's." |
| 1:55–2:15 | **Guardrail 1: over-limit refused** | Type `jio 9876543210 500`. | "Top-ups are ₹10 to ₹50 each. Pick an amount in that range." Say: "The model only reads my message. Code checks the limit: ₹50 an order, ₹100 a user a day, ₹200 a day for the whole bot. Nobody can approve more." |
| 2:15–2:35 | **Guardrail 2: kill switches** | Type `/stop 1`, then switch to the laptop: set `SSA_PAUSED=1` in `.env`, save, and tap ✅ Buy on a fresh quote (`jio 9876543210 10`). | "🛑 Auto-pay #1 stopped." Then "Purchases are paused by the operator right now." Say: "Two switches. The user stops a rule. The operator pauses everything, and the flag is re-read on every tap, so no restart." |
| 2:35–2:55 | **Honest trade-offs** | Talk over the README or `PRIVACY.md` open on the laptop. | Say: "It's testnet and custodial. We hold the keys, and Bitrefill is paid from a balance we prefunded, so your ZEC is a budget, not the payment. We see everything. Telegram, Anthropic and the light-wallet server each see something. Refilling that balance from ZEC deshields one transfer. It's all written down in PRIVACY.md." |
| 2:55–3:00 | **Close** | Repo link on screen. | `github.com/Vishal4742/shielded-shopping-agent` and `@zcashdemo_bot`. |

If you run over, shorten the kill-switch beat to `/stop 1` only and drop `SSA_PAUSED`. Keep `/report`: it is the proof for the Privacy judges.

## For the judge who reads the repo (not in the 3 minutes)

```sh
SSA_AUDIT_UFVK='uviewtest1…' cargo run --release -- audit <BIRTHDAY_HEIGHT>
```

Prints the same CSV for an outside auditor. Details are in the README.

## Pre-demo checklist

- [ ] **Wallet funded.** `/balance` shows enough testnet ZEC for the ₹10 quote plus the network fee on the quote card (25,000 zat). Faucet funds need about 10 confirmations before they count, so fund early.
- [ ] **Bitrefill balance funded.** The account balance is what pays for the top-up. If it reads 0, ask a Bitrefill mentor for event credits.
- [ ] **`BITREFILL_DRY_RUN=0`** in `.env` for the take. The default in `.env.example` is `1`, which buys nothing and answers "Dry run: nothing was bought and your budget is restored." Rehearse with `1`. Bitrefill allows **5 phone top-ups per account per day**, so spend at most one or two real ones.
- [ ] **Restart the bot after editing `.env`** (`cargo run --release`). Only `SSA_PAUSED` is re-read live. Set it back to `0` after the kill-switch beat.
- [ ] **`SSA_TOR` choice.** `SSA_TOR=1` sends wallet sync, receipts and account creation over Tor: about 11 s per call instead of 2 s, and the light-wallet server then sees a Tor exit, not your IP. For the take, use `SSA_TOR=0` so the receipt lands sooner; if you want Tor in the story, say it in the voice-over. `/report` and `ssa audit` connect directly either way.
- [ ] **Invite code and account ready.** `cargo run --release -- invite DEMO 10`, and your Telegram account has already done `/start DEMO` so the wallet exists. Don't spend video time on signup.
- [ ] **`/rules` is empty,** so the rule in the script is #1. Max 3 rules per user.
- [ ] **Under the daily limits.** You have not already spent ₹100 today (IST day), and the bot is under ₹200.
- [ ] **A receipt is already on chain.** Make one purchase at least 5 minutes before the take. The live run's receipt may not be mined by the time `/report` runs; the earlier one will be, so the CSV is not empty.
- [ ] **Phone recording:** screen recording on, Do Not Disturb on, clock visible, 1080p or better. Blur the `uviewtest1…` key in the edit. Never show `.env`, the Bitrefill key or a redemption code.
- [ ] **Capture the invoice ID** after the live purchase: `sqlite3 data/ssa.sqlite "select id, bitrefill_invoice from orders order by rowid desc limit 1"`, or the Bitrefill dashboard. Paste it into the README ("Invoice ID of a real purchase") and `SUBMISSION.md`. Chat does not show it.
- [ ] **Export under 3:00.** Upload it and open the link in a logged-out browser before pasting it into Stadium.
