# Stadium submission answers

Paste-ready. Anything marked `TODO` is a value we don't have yet: fill it in or delete the line before you finalize (deadline 20:00 IST, 4 Nov).

## Project

**Name:** Shielded Shopping Agent

**Tagline:** A Telegram agent that buys your phone top-ups on a schedule from a shielded Zcash budget, inside limits it can't override.

**Repo:** https://github.com/Vishal4742/shielded-shopping-agent
**Bot:** https://t.me/zcashdemo_bot (invite only; code in the demo notes)
**Landing page:** https://shielded-shopping.buildday-qr.workers.dev

**Demo video:** TODO link (script: [DEMO.md](DEMO.md))

## Description (about 150 words)

Shielded Shopping Agent is a Telegram bot, written in Rust, that buys Indian mobile top-ups (Jio, Airtel, Vi) for you on a schedule. You approve a rule once, for example `/auto jio 9876543210 10 weekly`. After that the agent quotes, checks its limits and pays through Bitrefill without another prompt, and warns you a minute before each run with a Skip button. Your budget is shielded Zcash testnet funds. Every purchase leaves an encrypted receipt memo on chain that only your viewing key can read, and `/report` rebuilds your purchase list from that key alone, in a throwaway watch-only wallet. Limits live in code, not in the language model: ₹50 an order, ₹100 a user a day, ₹200 a day overall, and nobody can approve more. It is testnet and custodial, and the trade-offs are written down in PRIVACY.md.

## Tracks, and which part is for which

We are entering both.

**Privacy (Zcash)**
- Deposits go to a shielded unified address per user (Ironwood pool, NU7 testnet).
- Each purchase writes an encrypted memo receipt, `SSA·<order id>·<operator>·₹<amount>`, in a shielded transaction.
- Viewing-key disclosure: `/viewkey`, `/report` (CSV of purchases rebuilt from the key alone in a throwaway watch-only wallet; verified on testnet to show the user's receipt and none of the treasury's transactions), and `ssa audit` for an outside auditor.
- Optional Tor for wallet traffic: `SSA_TOR=1` (about 11 s a call instead of 2 s).
- Trust note: [PRIVACY.md](PRIVACY.md), condensed below.

**Agentic Commerce (Bitrefill)**
- Bitrefill REST API v2: product lookup, invoice creation paid from the account balance, invoice polling until delivered.
- Agent-initiated purchases: auto-pay rules (`/auto`, `/rules`, `/stop`) run on a timer with no prompt at run time. Manual buying (`jio 9876543210 10`, then a ✅ tap) also works.
- Guardrails in code: [GUARDRAILS.md](GUARDRAILS.md).
- Refilling the Bitrefill balance from shielded ZEC with `ssa refill` (lane 5, Private by default), described below.
- Lanes: 1 (Rails and guardrails), 3 (No prompt required), 5 (Private by default).

## Bitrefill

### Touchpoints, built vs reused

We call the Bitrefill REST API v2 directly from one module, `src/bitrefill.rs`, with three endpoints: `GET /products/{slug}` for the package list and price, `POST /invoices` with `payment_method: "balance"` and `auto_pay: true` to buy, and `GET /invoices/{id}` polled until the status is delivered (paid is not the same as delivered). We don't use the MCP server or the CLI. Prices on our account come back in BTC satoshis, so we convert them to USD cents with a BTC/USD price from NEAR 1Click, and to zat with its ZEC/USD price. **Built by us:** the Bitrefill client and response parsers, the quote maths, the order state machine, the caps, the auto-pay scheduler, the ledger, the receipt queue, the viewing-key report, and the `ssa refill` command. **Reused:** the Bitrefill API itself, `zcash-devtool` for the wallet, NEAR 1Click for prices and for the ZEC refill swap, Telegram, and Anthropic Claude (to read the user's message; the LLM can't buy anything).

### Guardrails

- **The most an agent can spend:** ₹50 per order, ₹100 per user per day, ₹200 per day for the whole bot, at least 30 s between a user's purchases. Jio, Airtel and Vi only. Auto-pay: at most 3 rules per user.
- **Who approves above that:** nobody. There is no override button and no admin command. Raising a limit means changing a constant in the code and redeploying.
- **Who approves what is allowed:** the user taps ✅ on every manual quote, and taps ✅ Allow once per auto-pay rule. Each run is warned about a minute ahead with Skip and Stop buttons and re-checks every limit.
- **Kill switches:** `/stop <n>` for the user, `SSA_PAUSED=1` in `.env` for the operator (read on every purchase, no restart).

### Invoice ID of a real purchase

Invoice ID: TODO after the live purchase

### Feedback

[FEEDBACK.md](https://github.com/Vishal4742/shielded-shopping-agent/blob/main/FEEDBACK.md)

Repo: https://github.com/Vishal4742/shielded-shopping-agent

### Refilling the balance from ZEC

Bitrefill doesn't take ZEC, so the balance is prefunded. `ssa refill <USD> <usdc-base|btc> <BITREFILL_DEPOSIT_ADDR> <ZEC_REFUND_ADDR>` asks NEAR Intents 1Click for an exact-output quote (ZEC in, USDC on Base or BTC out, delivered to the Bitrefill deposit address) and prints a Zcash deposit address, the exact ZEC amount and a ZIP-321 payment URI. `ssa refill status <DEPOSIT_ADDR>` tracks the swap. The operator sends the ZEC from their own wallet; the command doesn't send it. It's mainnet only (1Click has no testnet), and it deshields that one transfer, because 1Click takes ZEC at a transparent address. Tested so far: a live dry quote for $5 of USDC on Base came back at about 0.00388 ZEC (fixture `fixtures/refill/quote-dry.json`). Live refill: TODO, not run yet.

## Privacy trust note (condensed from [PRIVACY.md](PRIVACY.md))

**What stays private.** Deposits and receipts are shielded Zcash transactions: a chain observer can't see amounts, sender, receiver or memos. Bitrefill never sees the user's Zcash address or Telegram account, because we pay from our own prefunded balance. The public stats page shows aggregates only.

**What the app reveals, and to whom.**
- *Us, the operator:* everything. Telegram id, phone numbers, every order, balances and keys. This is a custodial wallet.
- *Bitrefill:* the phone number, operator and amount of each top-up, tied to our account.
- *Telegram:* the whole chat. *Anthropic:* the text we send for intent parsing.
- *The light-wallet server (testnet.zec.rocks):* our IP and when we sync, unless `SSA_TOR=1`, when it sees a Tor exit. Tor is off by default and costs about 9 s extra per call. `/report` and `ssa audit` connect directly.
- *1Click and the chain, on a ZEC refill:* the amount and the Bitrefill deposit address of that one transparent transfer.
- *Whoever holds a viewing key:* every receipt of that account, including the order ids, amounts and the account's deposits. Give it only to someone you want to see that.
- *People in the room:* the stats page updates live, so a purchase made during a demo shows up as it happens.

**What you must trust.** Us, to hold the spending keys honestly. Bitrefill, to deliver. Telegram and Anthropic, with the chat text. The light-wallet server, for a correct view of the chain.

**Known gaps.** Memos aren't authenticated: someone who sends the user a memo in the `SSA·…` format would show up in a `/report`, so a report is a readout, not proof of purchase. An auditor's machine shows the key in `ps` during `init-fvk`, because devtool takes it only as an argument. All users share one wallet.

**What we'd fix next.** See below.

## What we'd do next

1. **Non-custodial.** Users hold their own keys and pay per order from their own wallet, so we only see that order's payment.
2. **Close the refill loop.** Today `ssa refill` prints the deposit address and the operator sends the ZEC by hand. Next: send it from the bot's own mainnet wallet and credit the Bitrefill balance when `ssa refill status` reports success.
3. **Authenticated receipts.** Check the memo's sender, so a forged `SSA·` memo can't appear in a report.
4. **Network privacy.** Tor or Nym for audits and the other direct connections; let the auditor pass the key without putting it in `ps`.
5. **No LLM.** A fixed grammar for "₹20 Jio to 98…", so no chat text leaves for Anthropic.
6. **Keep less.** Drop full phone numbers from our database once an order is delivered.
