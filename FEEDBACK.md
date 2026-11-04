# Bitrefill feedback

What confused or slowed us down while building on the Bitrefill API at the Free Society Hackathon (4 Nov 2026). We'll add to this list as we go.

## Account and balance

- **The account currency defaults to BTC, and catalog prices follow it.** `GET /v2/accounts/balance` returned `{"balance":0,"currency":"BTC"}`, and every package `price` is an integer in satoshis that drifts with the BTC rate (₹10 Jio talktime was 141 sat). `?currency=USD` is ignored. We convert sats → USD cents with a BTC/USD price from NEAR 1Click; a USD price field on the package would remove that hop.
- **The balance unit isn't documented.** With `currency: "BTC"`, is `balance` in BTC, sats or something else? We couldn't tell until the account was funded.

## Products and prices

- **The builder pack's package shape doesn't match the REST API.** It documents `package_value` / `payment_price`; the v2 product JSON has `{id, value, price, amount}`. `value` is the exact string to buy with (e.g. `₹7.47 Talktime Top-Up`, en dashes in Jio plan names), `amount` is the rupee face value. Also, Airtel on our account only had ₹500 and ₹1000 packages, so the "Airtel ₹10" demo tip didn't apply; Jio ₹10 did.
- **No reference or idempotency field on `POST /invoices`.** We can't attach our order id, so crash recovery can't look an invoice up by our id; we guard double purchases in our own DB instead.
- **`package_value` must match exactly.** Matching is case-sensitive, and some Jio package names contain en dashes (–), not hyphens.
- **Test products are hidden by default.** `test-gift-card-code` exists but only shows up after Bitrefill enables it for the account, so we couldn't test a full purchase for free on day one.

## Limits

- **5 phone top-ups per account per day.** We learned this from the event builder pack, not from the API. It would help if the API exposed the remaining count so a client could check before buying.

## Payments

- **No ZEC rail.** Bitrefill doesn't accept ZEC, and NEAR 1Click (the bridge route) has no testnet. We prefund a USD balance instead.

## Still to fill in

- [ ] Invoice flow: anything surprising about paid vs delivered and polling
- [ ] Whether invoices accept our own order reference
- [ ] Rate limits we hit, if any
- [ ] Error messages that were unclear
