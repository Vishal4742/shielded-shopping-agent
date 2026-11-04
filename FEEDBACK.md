# Bitrefill feedback

What confused or slowed us down while building on the Bitrefill API at the Free Society Hackathon (4 Nov 2026). We'll add to this list as we go.

## Account and balance

- **The account currency defaults to BTC, and catalog prices follow it.** `GET /v2/accounts/balance` returned `{"balance":0,"currency":"BTC"}`, and every package `price` is an integer in satoshis that drifts with the BTC rate (₹10 Jio talktime was 141 sat). `?currency=USD` is ignored. We convert sats → USD cents with a BTC/USD price from NEAR 1Click; a USD price field on the package would remove that hop.
- **The balance unit isn't documented.** With `currency: "BTC"`, is `balance` in BTC, sats or something else? We couldn't tell until the account was funded. After a 6,619-sat top-up it read 7,689, so we treat it as sats.

## Products and prices

- **The builder pack's package shape doesn't match the REST API.** It documents `package_value` / `payment_price`; the v2 product JSON has `{id, value, price, amount}`. `value` is the exact string to buy with (e.g. `₹7.47 Talktime Top-Up`, en dashes in Jio plan names), `amount` is the rupee face value. Also, Airtel on our account only had ₹500 and ₹1000 packages, so the "Airtel ₹10" demo tip didn't apply; Jio ₹10 did.
- **No reference or idempotency field on `POST /invoices`.** We can't attach our order id, so crash recovery can't look an invoice up by our id; we guard double purchases in our own DB instead.
- **`package_value` must match exactly.** Matching is case-sensitive, and some Jio package names contain en dashes (–), not hyphens.
- **Test products are hidden by default.** `test-gift-card-code` exists but only shows up after Bitrefill enables it for the account, so we couldn't test a full purchase for free on day one.

## Limits

- **5 phone top-ups per account per day.** We learned this from the event builder pack, not from the API. It would help if the API exposed the remaining count so a client could check before buying.

## Payments

- **No ZEC rail.** Bitrefill doesn't accept ZEC, and NEAR 1Click (the bridge route) has no testnet. The bot's purchases draw on a balance we prefund (in satoshis, because our account currency is BTC). For topping that balance up from real ZEC we wrote `ssa refill` (below).

## NEAR Intents 1Click (for the ZEC refill)

Not a Bitrefill thing, but it's the other half of "pay Bitrefill from shielded ZEC", so we're putting it here.

- **A dry quote is easy.** `POST /v0/quote` with `"dry": true` returns a price with no deposit address and nothing reserved. We asked for $5 of USDC on Base (an `EXACT_OUTPUT` quote, `amount` in the token's smallest unit, so `5000000`) and got about 0.00388 ZEC back as the input, with the output exactly `5.0`. That let us check the maths before touching real funds. Our parser test runs against that saved response.
- **The status endpoint returns 404 until a deposit address exists.** `GET /v0/status?depositAddress=…` answers `{"message":"Deposit address … not found","statusCode":404}` for an address the service doesn't know. A dry quote never creates one, so you can't try the status call against a dry run, and you can't tell a typo from "not created yet". We treat any reply without a `status` field as an error and show its message. A "not found yet" state, or a way to look a quote up by id, would be nicer.
- **Exact-output is the right mode for paying a merchant.** We want Bitrefill to receive exactly the amount we ask for, and `EXACT_OUTPUT` does that. The ZEC we send is what moves, and the quote comes with `minAmountIn` and a slippage setting in basis points.
- **ZEC goes in at a transparent address.** That's the one place the flow leaves the shielded pool. We'd have liked to pay from a shielded address. We say so in PRIVACY.md instead.
- **The echoed `quoteRequest` has an `appFees` entry we didn't send.** We haven't chased where it comes from.
- **No testnet.** We can only try the quote and status calls on mainnet, with small real amounts. We ran a dry quote, then one live refill: 0.00519175 ZEC became 6,619 sats of BTC in the Bitrefill balance.
- **The minimum is higher for BTC than we expected.** 1Click wanted about $5 for a BTC swap, so we fund the balance once and spend it down over many ₹10 orders instead of refilling per order.

## Still to fill in

- [ ] Invoice flow: anything surprising about paid vs delivered and polling
- [ ] Whether invoices accept our own order reference
- [ ] Rate limits we hit, if any
- [ ] Error messages that were unclear
