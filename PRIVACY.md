# Privacy trust note

This is a testnet demo, built in a day. Read this to see who learns what when you use it.

## What stays private

- **On-chain activity.** Deposits into your budget and the receipt for each purchase are shielded Zcash transactions (Ironwood pool, unified addresses). A chain observer can't see the amounts, senders, receivers or memos.
- **Receipts.** Each purchase leaves an encrypted memo on chain, `SSA·<order id>·<operator>·₹<amount>`. It carries no phone number. You can read your history with your viewing key (`/viewkey`, `/report`), and anyone you choose to give it to can import it and check the record.
- **Your link to the top-up, from the merchant's side.** Your ZEC never reaches Bitrefill. We pay from a prefunded Bitrefill balance, so Bitrefill sees neither your Zcash address nor your Telegram account.
- **The public stats page.** `/stats.json` returns four fields: delivered orders, rupees spent today, receipts sent and whether purchases are paused. No Telegram ids, phone numbers, addresses, txids or per-order times. The server accepts at most 32 connections at once.

## What the app reveals, and to whom

| Who | What they see |
|---|---|
| **Us (the operator)** | Everything: your Telegram id, the phone number you top up, every order, your balance, and your keys. We run a custodial wallet. Privacy here is against the public and the merchant, **not** against us. |
| **Bitrefill** | The phone number, operator and amount of each top-up, tied to our account (not yours). |
| **Telegram** | Your chat with the bot, including the phone number and amounts you type. |
| **Anthropic** | The text of messages we send for intent parsing ("₹20 Jio to 98…"). |
| **testnet.zec.rocks** (lightwalletd) | Our server's IP address and when and how often we sync. With `SSA_TOR=1` it sees a Tor exit instead of our IP (see Tor below). `/report` and `ssa audit` always connect directly. |
| **NEAR Intents 1Click and the chain, on a ZEC refill** | The amount of that one transparent transfer, the deposit address, and the Bitrefill address it is swapped to (see Refilling below). |
| **Anyone holding your viewing key** | Every deposit and receipt of your account, past and future: amounts, memos, order ids. A viewing key can't be revoked. |
| **People in the room** | The stats page updates live, so a purchase made during the demo shows up as it happens. With one buyer present, that links the purchase to them. |

## Viewing-key disclosure: `/report` and `ssa audit`

`/report` builds a throwaway watch-only wallet from your viewing key and nothing else, syncs it, and sends you a CSV of the purchases its receipt memos prove (`txid,mined_height,order_ref,operator,inr,zat`). `ssa audit <BIRTHDAY_HEIGHT>` does the same for an outside auditor who has only the key, passed in the `SSA_AUDIT_UFVK` environment variable. We tested it on testnet: the watch-only wallet lists the user's own transactions and none of the treasury's. The wallet can't spend (no spending key is involved at any step) and is deleted afterwards.

What this does and doesn't show:

- **It proves what the key reveals. It doesn't reduce what we know.** For `/report`, we are the ones holding your key, so we run the check for you. It demonstrates that an outside party with the key sees your receipts and nothing else.
- **The key is a one-way door.** Whoever gets it can read that account's history, including deposits, for good.
- **Memos aren't authenticated.** The report keys on the memo text, and a Zcash memo can be sent by anyone who has your address. Someone who sends you a memo in the `SSA·…` format will show up in a report as a purchase. Treat a report as a readout of your wallet, not as proof that Bitrefill delivered. The fix is to check who sent the memo; we haven't built that.
- **The auditor's machine shows the key in `ps`** while `init-fvk` runs, because zcash-devtool takes the key only as a command-line argument (no stdin or file input). Run audits on a machine you control.
- **The audit connects directly, not over Tor.** The light-wallet server sees the sync from the account's birthday, and `enhance` fetches individual transactions, so it also sees which ones that wallet asks for.

## Refilling the Bitrefill balance from ZEC: one deshielded hop

Bitrefill doesn't take ZEC, so the balance we pay from is prefunded. `ssa refill` is our route to top it up from shielded ZEC, through NEAR Intents 1Click: ZEC in, USDC on Base (or BTC) out, delivered to a Bitrefill deposit address.

1Click takes ZEC at a **transparent** address, so that one transfer leaves the shielded pool:

- The amount and the deposit address are public on the Zcash chain.
- 1Click sees the swap, including the Bitrefill address it pays and our refund address.
- The Bitrefill address is on a public chain too (Base, or Bitcoin).
- It's our own refill, a single pooled transfer, not one per user. It doesn't name you or your purchases, but it does tell an observer that this operator funds Bitrefill with ZEC, and how much, when.
- It's mainnet only. 1Click has no testnet, so the demo bot's testnet ZEC never goes through it.
- We have done this once for real: 0.00519175 ZEC from a shielded wallet, swapped to BTC and credited to the Bitrefill balance as 6,619 sats (the balance then read 7,689 sats). 1Click's minimum for BTC was about $5. That transfer, its amount and the Bitrefill deposit address are public on the Zcash and Bitcoin chains.
- The command prints the address, the exact ZEC amount and a ZIP-321 payment URI. The operator sends the ZEC from their own wallet, so the sending side's privacy is whatever that wallet does.

## Tor: `SSA_TOR=1`

With `SSA_TOR=1` in `.env`, every wallet sync, receipt send and account creation goes through Tor (zcash-devtool's built-in `--connection tor`). The light-wallet server then sees a Tor exit, not our IP.

- **Off by default.** It costs time: about 11 s per call on NU7 testnet instead of 2 s, because each call starts its own Tor client. Deposits are still detected inside the 20 s sync loop, but each new wallet and each receipt takes about 10 s longer.
- **It covers wallet traffic only.** Telegram, Bitrefill, Anthropic and the NEAR price calls are plain HTTPS from our server, and `/report`, `ssa audit` and `ssa refill` don't use it.
- It hides the server's view of us. It doesn't hide anything from us, Bitrefill, Telegram or Anthropic.

## What you have to trust

- **Us**, to hold your spending keys honestly. All users share one devtool wallet, with one account each. We could spend your budget or read your receipts.
- **Bitrefill**, to deliver the top-up and handle the phone number properly.
- **Telegram and Anthropic**, with the chat text.
- **The lightwalletd server**, to give our wallet a correct view of the chain and not log our IP (or, with Tor on, our sync pattern).
- **NEAR Intents 1Click**, if the operator refills with ZEC, to deliver the swap or refund it.

## What we'd fix next

1. **Non-custodial.** Users hold their own keys and pay per order from their own wallet. We'd only see the payment for that order.
2. **Authenticated receipts.** Record who sent each memo and require it to be our treasury or the user's account, so a forged memo can't land in a report.
3. **Network privacy.** Put audits and the remaining direct connections behind Tor or Nym, and let an auditor pass the key without putting it in `ps` (needs a devtool change).
4. **No LLM.** Parse "₹20 Jio to 98…" with a fixed grammar, so no chat text leaves for Anthropic.
5. **Finish the ZEC refill.** `ssa refill` gets the quote and the deposit address; the bot should send the ZEC itself and credit the balance on success.
6. **Keep less.** Drop full phone numbers from our database once an order is delivered.
