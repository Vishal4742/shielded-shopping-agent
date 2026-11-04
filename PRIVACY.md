# Privacy trust note

This is a testnet demo, built in a day. Read this to see who learns what when you use it.

## What stays private

- **On-chain activity.** Deposits into your budget and the receipt for each purchase are shielded Zcash transactions (Ironwood pool, unified addresses). A chain observer can't see the amounts, senders, receivers or memos.
- **Receipts.** Each purchase leaves an encrypted memo on chain that records the order. You can read your history with your viewing key (`/viewkey`), and anyone you choose to give it to can import it and check the record.
- **Your link to the top-up, from the merchant's side.** Your ZEC never reaches Bitrefill. We pay from a prefunded Bitrefill balance, so Bitrefill sees neither your Zcash address nor your Telegram account.
- **The public stats page.** It shows aggregates only: no Telegram ids, phone numbers, addresses, txids or per-order times.

## What the app reveals, and to whom

| Who | What they see |
|---|---|
| **Us (the operator)** | Everything: your Telegram id, the phone number you top up, every order, your balance, and your keys. We run a custodial wallet. Privacy here is against the public and the merchant, **not** against us. |
| **Bitrefill** | The phone number, operator and amount of each top-up, tied to our account (not yours). |
| **Telegram** | Your chat with the bot, including the phone number and amounts you type. |
| **Anthropic** | The text of messages we send for intent parsing ("₹20 Jio to 98…"). |
| **testnet.zec.rocks** (lightwalletd) | Our server's IP address and when and how often we sync. |
| **People in the room** | The stats page updates live, so a purchase made during the demo shows up as it happens. With one buyer present, that links the purchase to them. |

## What you have to trust

- **Us**, to hold your spending keys honestly. All users share one devtool wallet, with one account each. We could spend your budget or read your receipts.
- **Bitrefill**, to deliver the top-up and handle the phone number properly.
- **Telegram and Anthropic**, with the chat text.
- **The lightwalletd server**, to give our wallet a correct view of the chain and not log our IP.

## What we'd fix next

1. **Non-custodial.** Users hold their own keys and pay per order from their own wallet. We'd only see the payment for that order.
2. **Network privacy.** Route lightwalletd traffic through Nym so the server can't tie syncs to our IP.
3. **No LLM.** Parse "₹20 Jio to 98…" with a fixed grammar, so no chat text leaves for Anthropic.
4. **Automatic refill on mainnet.** Top up the Bitrefill balance from shielded ZEC through NEAR 1Click (it has no testnet today).
5. **Keep less.** Drop full phone numbers from our database once an order is delivered.
