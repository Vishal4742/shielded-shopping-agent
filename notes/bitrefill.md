# Bitrefill findings (P0.6), 2026-10-30

Auth: `Authorization: Bearer $BITREFILL_API_KEY` against `https://api.bitrefill.com/v2` works (EI-3 confirmed).

| Call | Result |
|---|---|
| `GET /v2/ping` | `200 {"meta":{"_endpoint":"/ping"},"message":"pong"}` |
| `GET /v2/accounts/balance` | `200 {"meta":{"_endpoint":"/accounts/balance"},"data":{"balance":0,"currency":"BTC"}}` |

## Open
- [ ] Account currency is **BTC**, but the design assumes USD cents. Switch the account to USD in settings before funding, or handle BTC units in `quote`.
- [ ] Fund about $20, re-check balance (and the unit of `balance`: sats? cents?)
- [ ] Manual ₹10 Jio top-up
- [ ] GET product JSON for Jio/Airtel/Vi → fixtures
- [ ] Invoice reference field (Q5)
