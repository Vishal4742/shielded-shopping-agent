# Commerce and Telegram UX references

## How should the landing page lead into the Telegram purchase flow?

### Takeaway
Keep purchase activity in the Telegram private chat and use the public page to show one short, believable journey: request → quote → explicit confirmation → top-up result → shielded receipt. Telegram's own guidance favors inline buttons for yes/no actions tied to a message and editing that message as its state changes, which fits the repository's quote/confirm flow.

### Cited Findings
- Telegram supports deep links in the form `https://t.me/<bot_username>?start=<parameter>`; in a private chat Telegram passes the value to the bot as `/start <parameter>`. This can make the public page's Open in Telegram action carry the invite/demo start value when needed. [Telegram Bot Features](https://core.telegram.org/bots/features)
- Telegram's bot guidance says use `/start` for the first interaction, warns that bots often overuse commands, and recommends custom keyboards for limited known input and inline keyboards for actions that modify a specific existing message. [Telegram Bot Guidelines](https://core.telegram.org/bots/guidelines)
- Telegram's Bot API says a callback button shows a client-side progress bar until the bot calls `answerCallbackQuery`; it also supports editing an existing message's text and inline keyboard. [Telegram Bot API](https://core.telegram.org/bots/api)
- The Telegram menu button can present bot commands with descriptions near the message field, so `/balance`, `/viewkey`, and `/help` do not need to be re-explained in every bot response. [Telegram Bot Features](https://core.telegram.org/bots/features)
- Bitrefill describes its Personal API as intended for individual developers, personal automations, learning and small-scale purchases; it exposes catalog browsing, invoice creation and order tracking. [Bitrefill API overview](https://docs.bitrefill.com/docs/api-overview)

### Inferences
- The landing page should link straight to the bot and show a static QR as a parallel path, while purchase itself stays in Telegram as required by FR-15 and SRS §2.6.
- In the demo chat, attach ✅ Confirm and ❌ Cancel to the quote message, acknowledge the callback immediately, then edit that same message to “Buying…” and the final delivered/failed/uncertain state. This keeps the order's history readable and ties each action to the quoted amount.
- Show the exact quote fields already required by FR-7: INR amount, ZEC amount, fee, expiry, and masked recipient number. Keep state wording aligned with the bot's actual FR-9/FR-14 outcome; do not let an animated mock chat imply a successful purchase when the real state is uncertain.

### Gaps
- Telegram docs describe interaction primitives, not a visual style for a product-specific bot chat. The chat card should be checked in Telegram iOS and Android clients before treating spacing or button density as settled.
- This research did not verify which Telegram clients currently render a QR-scanned bot deep link with the same first-run presentation.

## How should the product, order status, and public stats be represented?

### Takeaway
Show the top-up as a concrete product and make order states legible as states, not as a single generic success animation. For public metrics, show freshness and definitions beside the numbers; do not imply that a testnet demo's activity measures network-wide Zcash adoption.

### Cited Findings
- Bitrefill's India storefront lists Jio, Airtel and Vi top-up ranges, then explains a three-step flow: enter an amount, pay, and use the result. Its category list exposes the local product range before detailed explanation. [Bitrefill India top-ups](https://www.bitrefill.com/send-recharge/india/?hl=en)
- Bitrefill's API defines invoice states including `unpaid`, `payment_detected`, `payment_confirmed`, `pending`, and `complete`; `complete` means all orders were delivered. Order states include `created`, `payment_detected`, `processing`, `delivered`, `failed`, and `refunded`. Phone top-ups may have no redemption code because the phone balance is credited directly. [Bitrefill Core Concepts](https://docs.bitrefill.com/docs/core-concepts)
- Bitrefill's integration guide lists phone number as a top-up input, says `auto_pay: true` immediately pays a balance invoice when funds suffice, and recommends polling for final states `complete`, `denied`, or `payment_error` if webhooks are unsuitable. Its error table explicitly says to show an `out_of_stock` error to the user. [Bitrefill integration flow](https://docs.bitrefill.com/docs/integration-flow)
- Bitrefill's India page surfaces operator denominations and includes a short three-step explanation, but its marketing line “confirmed the same minute in most cases” is a Bitrefill storefront claim, not a guarantee for this bot's end-to-end 15-second target. [Bitrefill India top-ups](https://www.bitrefill.com/send-recharge/india/?hl=en)
- ZecZcash's shielded-supply dashboard pairs its figure with an observation date and block height, explains that the number is a pool-balance snapshot rather than transaction volume or individual activity, and links its methodology and source. [Zcash Shielded Supply Tracker](https://zeczcash.com/shielded)
- ZecStats' live dashboard shows a “Last updated” timestamp and a compact set of labeled network indicators. It is an independent dashboard and not an official Zcash source. [ZecStats](https://zecstats.org/)

### Inferences
- On the public page, make the hero's proof a real-looking, clearly marked demo quote: “Jio · ₹10 · 0.000… ZEC + fee · quote expires in 10 min”, with the masked number and confirmation controls visible. The primary action remains “Open in Telegram”; the demo does not turn the landing page into a buying UI.
- Separate quote, purchase, and receipt states. “Quote ready”, “Buying”, “Top-up delivered”, “Checking with Bitrefill”, and “Receipt sent” communicate distinct outcomes and map to the repository's FR-7, FR-9, FR-10 and FR-14.
- For FR-16, label all displayed counters “Zcash testnet” and show update time. Keep `₹ committed today / ₹200 cap`, delivered top-ups, and shielded receipts separate; describe them as this demo's activity, not as user adoption or total Zcash usage. On stale or failed fetch, replace the counts with the specified offline state.
- Do not add checkout forms, account views, or a receipt viewer to the landing page: the repository explicitly keeps buying in Telegram and excludes a web/mobile purchase or receipt-viewing app.

### Gaps
- The Bitrefill public storefront does not establish how the Personal API behaves for this project's specific Indian operators, account balance, or live timing. Keep the bot's copy bounded by behavior verified through its own fixtures and end-to-end demo.
- The Zcash stats dashboards measure public network data with different methods. They are references for labeling and freshness, not data sources for FR-16's app-specific counters.
