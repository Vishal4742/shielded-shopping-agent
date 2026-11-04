# Security

This bot spends real money (Bitrefill store credit) and holds users' testnet wallet keys, so please report problems privately.

## Reporting

Use GitHub's **Report a vulnerability** button (Security tab → Advisories), or email the maintainer listed on the GitHub profile. Please don't open a public issue for anything that could move money, leak a key or a phone number, or let one user act on another's order or auto-pay rule.

We'll acknowledge within 48 hours during the hackathon window (4–5 Nov 2026) and within a week after.

## What's in scope

- Buying past the limits in `src/caps.rs` (₹10–50 per order, ₹100/user/day, ₹200/day for the whole bot, 30 s between buys), double purchases, or a purchase without the owner's ✅ or an approved auto-pay rule.
- Leaks of `.env` secrets, viewing keys (UFVK), seeds, Bitrefill redemption codes or full phone numbers into logs, messages or the public `/stats.json`.
- Crediting a deposit to the wrong user or twice.

## Known limits (documented, not bugs)

See [PRIVACY.md](PRIVACY.md) and [GUARDRAILS.md](GUARDRAILS.md): the bot is custodial (one shared devtool wallet), it's testnet only for user funds, receipt memos aren't authenticated, and an order with no Bitrefill invoice id stays `UNCERTAIN` until the operator checks.

## Operator checklist

- Keys live only in `.env` (gitignored); rotate the Bitrefill key and bot token after the event.
- `SSA_PAUSED=1` in `.env` stops all purchases at the next ✅ or auto-pay run, without a restart.
