# Money flow references

## How to communicate a hold versus a completed charge

### Takeaway
Give each money state its own label and explain what happened to the funds. A reserved amount, a posted charge, a refund, and a bank-side balance update are different events; one compact timeline can make that distinction legible.

### Cited Findings
- Airbnb explains that a payment hold reserves funds but is not yet the completed charge; it says the hold converts to a charge after acceptance, while cancellation before charge removes the hold. A released hold can take up to seven business days to show in the payment provider’s account. [Airbnb payment holds](https://www.airbnb.com/help/article/3549)
- Stripe’s explainer names the states in sequence: authorization request, issuer reservation, pending transaction, capture, posted charge, and release of an uncaptured hold. It notes that a hold reduces what the customer can spend even though funds have not moved to the business. [Stripe authorization holds](https://stripe.com/resources/more/authorization-holds-explained)
- Airbnb’s monthly-stay guidance separates the guest charge from host payout: first-month payment is charged up front, but released to the host after check-in; it also points customers to the scheduled date and amount under upcoming payments. [Airbnb monthly-stay payments](https://www.airbnb.com/help/article/245)

### Inferences
- For Tiffin Pass, show “Reserved” and “Charged” as visibly different states, with the amount and trigger for each. Avoid using “escrow” unless the product’s legal and payment structure actually qualifies as escrow.
- A payment timeline should show customer debit and kitchen payout as separate events. If payout occurs before the meal is collected, the UI needs a short explanation of how that is funded.

### Gaps
- These help pages explain payment mechanics but do not expose the full current checkout UI or establish that Tiffin Pass can legally or operationally use the same funds flow.

## How to show refunds and seller payouts

### Takeaway
Strong payout and refund experiences show the amount, status, destination, and expected timing together. They also let people see the refundable amount before they commit to canceling.

### Cited Findings
- Airbnb lets a guest review a detailed refund breakdown before canceling and later open refund status details to review the amount. Its help page says Airbnb initiates an eligible refund at cancellation, while the bank or payment provider controls when it arrives. [Airbnb refund amount and status](https://www.airbnb.com/help/article/311)
- Etsy’s seller payment account has a Pending tab; an order can show the amount held in reserve and its expected availability date. Expanding an order shows how much is available, pending, or held. [Etsy payment reserves](https://help.etsy.com/hc/en-us/articles/360058722214-What-is-a-Payment-Account-Reserve)
- Square’s transfer details show Pending, Completed, or Failed, the destination account’s last three digits, and an estimated arrival date and time; the detail view can also list the card payments included in a transfer. [Square transfer status](https://squareup.com/help/us/en/article/8328-troubleshoot-missing-transfers)
- Airbnb’s scheduled-payment guidance says a confirmed reservation gets an emailed payment schedule and customers can view upcoming charge dates and amounts in their payment details. [Airbnb scheduled payments](https://www.airbnb.com/help/article/2143)

### Inferences
- Put refund eligibility and the amount before the cancellation action; after it, keep an item-level status with “initiated” separate from “received.”
- For a kitchen payout card, show the gross lunch price, the kitchen amount, the route share, payout status, and expected date in one expandable detail. Keep an ETA visibly marked as an estimate.
- State the refund clock’s start event explicitly (“7 days after pass expiry” or another true trigger), and distinguish the platform’s send date from the bank’s arrival date.

### Gaps
- The cited sources document their own rules and interfaces. Their timelines and hold arrangements should not be copied as Tiffin Pass promises.

## How to show delivery progress

### Takeaway
Use a small set of named order states, then reveal live courier/map detail only after a courier is assigned. That keeps early order status useful without pretending the delivery route is already known.

### Cited Findings
- Uber Eats describes an order progress sequence with statuses such as order received, heading to store, shopping in progress, heading your way, and almost here. It says the map appears once a delivery person is selected. [Uber Eats order tracking](https://help.uber.com/en/ubereats/stores/article/check-the-status-of-my-order-?nodeId=14ad371a-cc67-408c-8d62-34bf738dc8c6)
- Uber’s help page tells customers to open the current order, tap Track, read its status description, and then follow the assigned courier’s progress. [Uber Eats order status steps](https://help.uber.com/ubereats/restaurants/article/se-status-p%C3%A5-min-bestilling?nodeId=01d2f4cc-3176-40d6-bf9e-1588e3e83408)

### Inferences
- Tiffin Pass’s ordered timeline can use the same staged pattern: menu picked → kitchen paid / cooking → dabbawala collected → on route → at desk. Pair each stage with its timestamp or next expected event, and only add location tracking if it is real and available.
- Keep the payment state in the same receipt/timeline as the delivery state so users can see when an amount was reserved, charged, and paid out relative to fulfillment.

### Gaps
- Uber’s app states are documented in help text; the article does not show the current visual treatment or prove that map tracking is necessary for a neighborhood lunch route.
