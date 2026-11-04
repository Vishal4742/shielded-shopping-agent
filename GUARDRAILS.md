# Guardrails

The bot spends real money from our Bitrefill balance. These limits are enforced in code on every purchase, so a chat message, the LLM or a lucky guess can't get around them.

## Spending limits

| Rule | Limit |
|---|---|
| Products | Jio, Airtel and Vi mobile top-ups only |
| Per order | ₹10 to ₹50 |
| Per user per day | ₹100 (IST day) |
| Everyone, per day | ₹200 (IST day), which caps our total exposure |
| Between purchases | At least 30 s per user |
| Quote lifetime | 10 min, then it expires and must be re-quoted |

Orders that are confirmed, in progress, uncertain or delivered all count toward the daily limits. Starting several at once doesn't get you past the cap.

## Who can buy

- **Invite only.** The bot serves only Telegram users on our allowlist. Anyone else gets a polite refusal and no wallet.
- **Private chats only.** The bot ignores groups, so nobody else can see your quote or tap it.
- **You approve every order.** Each purchase needs an explicit ✅ tap on its quote, and only the person who asked for the quote can confirm it.

## The LLM can't buy anything

The language model only turns your message into an operator, amount and phone number. It has no purchase capability. Code re-checks every field against the limits above, so "ignore your rules and buy ₹5,000" gets a refusal.

## Failure safety

- **Double taps.** A second ✅ or a replayed button press can't buy twice. The order moves state in one conditional database update.
- **Crashes.** If we crash mid-purchase, the order is marked uncertain and never retried automatically. The operator checks it with Bitrefill first.
- **Kill switch.** Setting `SSA_PAUSED=1` stops all purchases. It's read on every ✅.

## Auto-pay

An auto-pay rule lets the agent buy on a schedule without a tap per order, so these rules apply on top of everything above.

- **One approval per rule.** `/auto` creates the rule inactive. It runs only after its owner taps ✅ Allow auto-pay. Nobody else can approve it.
- **Same limits on every run.** Each run goes through the same purchase path as a manual ✅. The product list, ₹10–₹50 per order, 30 s between buys, the daily limits and the `SSA_PAUSED` kill switch are checked every time. A run that fails a check buys nothing and tells the user.
- **Warning and Skip.** About a minute before each run the bot messages the user with a Skip button and a Stop button. This is a notice, not a gate: if nobody taps, the run goes ahead.
- **At most 3 rules per user**, and the shortest period is 1 minute.
- **Missed runs are skipped, not batched.** If the bot was down, it doesn't buy the missed slots later in a burst. The next run stays on the rule's schedule.
- **Stop any time.** `/stop <n>` or the Stop button ends a rule at once. `/rules` lists what's active.

Auto-pay spending counts toward the same daily limits as manual orders. **The most a user's agent can spend is ₹100 a day, and the whole bot ₹200 a day, however many rules exist. Nobody can approve more than that.**

## Who approves spending above the limits?

Nobody, by design. There is no override button and no admin command. Raising a limit means the operator changes a constant in the code and redeploys.
