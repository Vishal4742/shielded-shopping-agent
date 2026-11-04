# Shielded Shopping Agent

A Telegram bot, written in Rust, that tops up Indian phones (Jio, Airtel, Vi) for you on a schedule. You approve a rule once, for example `/auto jio 9876543210 10 weekly`, and the agent then buys real top-ups through Bitrefill while you sleep. Your budget is shielded Zcash testnet funds. Every run passes the same limits in code, and every purchase leaves a shielded receipt memo that only your viewing key can read.

Built for the Free Society Hackathon, Mumbai, 4–5 Nov 2026. Entered in both the **Privacy (Zcash)** and **Agentic Commerce (Bitrefill)** tracks. Try it: [@zcashdemo_bot](https://t.me/zcashdemo_bot) (invite only).

## How it works

1. **Deposit.** Join with an invite code. The bot gives you your own shielded address (and a QR code). Send it testnet ZEC. That balance is your budget.
2. **Approve a rule once.** `/auto jio 9876543210 10 weekly`, then tap ✅ Allow auto-pay. (Manual buying still works: type `jio 9876543210 10`, read the quote, tap ✅ Buy.)
3. **The agent buys on schedule.** A minute before each run it messages you with a Skip button. Then it quotes the price, checks the limits, creates a Bitrefill invoice and polls it until the top-up is delivered.
4. **You get a shielded receipt.** A shielded transaction with an encrypted memo records the order. Read it with `/viewkey`, or give that key to an auditor.

## Which part is for which track

| Track | What counts |
|---|---|
| **Privacy (Zcash)** | Shielded deposits (unified addresses, Ironwood pool). Encrypted receipt memos for each purchase. Viewing-key disclosure with `/viewkey`. Optional Tor for wallet traffic with `SSA_TOR=1`. A trust note on who sees what: [PRIVACY.md](PRIVACY.md). |
| **Agentic Commerce (Bitrefill)** | Bitrefill REST API v2: product lookup, invoices, invoice polling. Agent-initiated purchases on a schedule, with no prompt from the user at run time. Code-enforced guardrails: [GUARDRAILS.md](GUARDRAILS.md). |

Bitrefill lanes we're going for:

- **Lane 1, Rails and guardrails.** Spend caps, a kill switch and a confirm step live in code, not in a prompt.
- **Lane 3, No prompt required.** Auto-pay rules run on a timer without anyone typing a request.
- **Lane 5, Private by default.** Funds and receipts are shielded Zcash. Bitrefill never sees the user's address or Telegram account.

## Run it in 15 minutes

You need Rust 1.99 or newer, git, a Telegram bot token from @BotFather, a Bitrefill API key and an Anthropic API key.

```sh
# 1. Build the Zcash wallet tool at the commit we tested
git clone https://github.com/zcash/zcash-devtool tools/zcash-devtool
(cd tools/zcash-devtool && git checkout 0b3436b && cargo build --release)

# 2. Create the testnet wallet. Account 0 is the treasury that receives receipts.
D=tools/zcash-devtool/target/release/zcash-devtool
$D wallet -w tools/wallets/ssa init --name treasury -i tools/wallets/identity.age -n test
$D wallet -w tools/wallets/ssa list-accounts          # note the treasury account's UUID
$D wallet -w tools/wallets/ssa list-addresses <UUID>  # copy its unified address (utest1…)

# 3. Configure
cp .env.example .env      # fill in TELEGRAM_BOT_TOKEN, BITREFILL_API_KEY, ANTHROPIC_API_KEY, SSA_TREASURY_UA

# 4. Create an invite code (DEMO, 10 uses), then start the bot
cargo run --release -- invite DEMO 10
cargo run --release
```

`.env.example` defaults to `BITREFILL_DRY_RUN=1`, which creates no invoices and buys nothing. Set it to `0` for real purchases. Bitrefill limits an account to 5 phone top-ups a day.

Then in Telegram:

1. Send `/start DEMO`. You get a shielded address and a QR code.
2. Send testnet ZEC to it from a faucet. The bot checks for deposits every 20 s. `/balance` shows what you can spend.
3. Send `jio 9876543210 10`, read the quote and tap ✅ Buy.
4. Send `/auto jio 9876543210 10 every 2m` and tap ✅ Allow auto-pay. The first run comes 2 minutes later, preceded by a warning with Skip and Stop buttons.
5. `/rules` lists your rules, `/stop 1` ends rule 1, `/viewkey` shows your viewing key.

To pause all buying, set `SSA_PAUSED=1` in `.env`. It is read again on every purchase, so no restart is needed. Tests: `cargo test`.

## Bitrefill touchpoints

We call the Bitrefill **REST API v2** directly from one module (`src/bitrefill.rs`) and use three endpoints: `GET /products/{slug}` for the package list and price, `POST /invoices` with `payment_method: "balance"` and `auto_pay: true` to buy, and `GET /invoices/{id}` polled until the status is delivered (paid is not the same as delivered). We don't use the MCP server or the CLI. Prices on our account come back in BTC satoshis, so we convert them to USD cents with NEAR 1Click's BTC/USD price and to zat with its ZEC/USD price. **Built by us:** the Bitrefill client and response parsers, the quote maths, the order state machine, the caps, the auto-pay scheduler, the ledger and the receipt queue. **Reused:** the Bitrefill API itself, `zcash-devtool` for the wallet, NEAR 1Click for prices, Telegram and Anthropic Claude (to read the user's message). Notes on what confused us: [FEEDBACK.md](FEEDBACK.md).

## Guardrails

Enforced in code on every purchase, manual or scheduled. Full list: [GUARDRAILS.md](GUARDRAILS.md).

- Jio, Airtel and Vi only. ₹10–₹50 per order. 30 s between buys.
- ₹100 per user per day, ₹200 per day for the whole bot. Nobody can approve more.
- Auto-pay: one approval per rule, at most 3 rules per user, a warning with Skip a minute before each run, stop any time. Missed runs are skipped, not bought later in a batch.
- Kill switch: `SSA_PAUSED=1`. The LLM only reads the message. It can't buy anything.

## Privacy and trust

Deposits and receipts are shielded, and Bitrefill sees our account, not yours. We run a custodial wallet, so privacy here is against the public and the merchant, not against us. Telegram, Anthropic and the lightwalletd server each see something too. The full table and our fix list are in [PRIVACY.md](PRIVACY.md).

## Invoice ID of a real purchase

`TODO: fill after the live ₹10 Jio purchase`

## Limitations

- **Custodial, shared wallet.** One devtool wallet with one account per user. The operator holds the spending keys.
- **Prepaid Bitrefill balance.** Bitrefill doesn't take ZEC, and NEAR 1Click has no testnet, so purchases draw on a balance we funded in advance. The user's ZEC is a budget in our ledger and the receipt, not the payment Bitrefill receives.
- **The LLM sees chat text.** Messages go to Anthropic to extract operator, phone and amount. A plain parser is the fallback.
- **Testnet only.** TAZ has no value.

## Project layout

- `src/main.rs`: starts the bot and workers; `ssa invite <CODE> <USES>` adds an invite code.
- `src/telegram.rs`: command handlers, invite allowlist, private-chat guard, button owner check.
- `src/autopay.rs`: auto-pay rules, schedule parsing, the warn-then-run loop.
- `src/orders.rs`: quote, confirm-and-buy, refund and crash recovery.
- `src/caps.rs`: spending limits (pure).
- `src/quote.rs`: price in zat, rounded up, with fee (pure).
- `src/order_fsm.rs`: legal order state changes (pure).
- `src/zip321.rs`: `zcash:` payment URIs for deposit QR codes (pure).
- `src/intent.rs`: text to operator, phone and amount; Claude first, parser fallback.
- `src/price.rs`: ZEC/USD and BTC/USD from NEAR 1Click.
- `src/bitrefill.rs`: Bitrefill REST client.
- `src/zec.rs`: runs `zcash-devtool` behind one wallet mutex and parses its output.
- `src/db.rs`, `migrations/`: SQLite for invites, users, ledger, orders and rules.
- `src/workers.rs`: deposit sync, receipt queue and the public `/stats.json` page.
- `src/config.rs`, `src/model.rs`: `.env` loading and shared types.

More detail: [ARCHITECTURE.md](ARCHITECTURE.md).

## Team workflow

Pure modules were written test first and take the clock as a parameter. Each change goes through a fresh-context code review and `cargo fmt && cargo clippy -- -D warnings && cargo test` before it is pushed. Money is integers only (zat, paise, cents). Secrets live in `.env`, which is gitignored. Commits look like `feat: description (FR-x)`. The code was written on 4 Nov, after the 11:00 start.
