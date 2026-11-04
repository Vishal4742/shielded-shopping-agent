# Zcash privacy and wallet UX references

## How can the page explain shielded privacy without overclaiming?

### Takeaway
Use a small “who can see what” explainer that distinguishes public-chain visibility from what the merchant, Telegram, the operator, and the user can see. Zcash-focused product pages make privacy accessible by pairing short benefit labels with concrete UI imagery; the SSA page should pair those cues with its own honest disclosure.

### Cited Findings
- Zcash’s current learning page describes a fully shielded transaction as hiding the sender, receiver, amount, and memo from the public blockchain; its wallet UX checklist likewise says the details of a shielded-to-shielded transaction are not revealed on-chain. [Zcash: Is Zcash traceable?](https://z.cash/learn/is-zcash-traceable/) · [Zcash Feature UX Checklist](https://zcash.readthedocs.io/en/latest/rtd_pages/ux_wallet_checklist.html)
- Zcash documentation distinguishes fully shielded transfers from transactions involving transparent addresses: transparent-side data is public, and the network fee remains visible. The docs explain the shielded/transparent difference with direct language instead of leading with cryptography terms. [Zcash: Addresses and Value Pools](https://zcash.readthedocs.io/en/latest/rtd_pages/addresses.html)
- The Zodl homepage opens with a short product headline and a download action, then organizes content under “Privacy,” “Self-Custody,” and “Consent.” Its later feature sections use small, named topics such as “Real-World Purchases,” “No Tracking,” and “Shielded Notes.” [Zodl](https://zodl.com/)
- Signal’s support page makes one unconditional claim (“always end-to-end encrypted”) and immediately follows it with the mechanism users can check, safety numbers. This is a useful copy structure only when the product can support the same unconditional claim. [Signal: Is it private? Can I trust it?](https://support.signal.org/hc/en-us/articles/360007320391-Is-it-private-Can-I-trust-it)
- Zcash’s public “Is Zcash traceable?” page uses a simple privacy analogy and links to related learning content, but it describes protocol behavior, not the full set of services involved in a purchase. [Zcash: Is Zcash traceable?](https://z.cash/learn/is-zcash-traceable/)

### Inferences
- Replace a universal “leave no trail” promise with a claim scoped to the chain, such as “The shielded receipt memo isn’t readable from the public blockchain.” The product’s own threat model says the operator sees user orders and full phone numbers, so the site should put the operator in the disclosure diagram rather than imply the viewing key is the only route to receipt data. [Zcash: Addresses and Value Pools](https://zcash.readthedocs.io/en/latest/rtd_pages/addresses.html) · repo: `docs/07-threat-model.md` §§1, 4
- A compact five-column interaction could show **Public chain · Bitrefill · Telegram · Operator · You**. Keep the page’s primary visual on one sample order; highlight only the fields each participant can access. This adapts the concrete, benefit-led structure on Zodl without borrowing its stronger self-custody or no-tracking claims. [Zodl](https://zodl.com/)
- Keep the first explanation nontechnical, then offer an optional “How shielded receipts work” link for Orchard/Ironwood, encrypted memos, and keys. Zcash’s own learning site separates beginner explainers from more advanced topics. [Zcash: What are zk-SNARKs?](https://z.cash/learn/what-are-zk-snarks/)

### Gaps
- The sources describe Zcash protocol behavior or Zodl/Signal products. They do not independently verify the SSA bot’s wallet, Bitrefill, Telegram, or operator data handling.

## How should the page present receipts and viewing keys?

### Takeaway
Make the receipt the privacy proof: show a sample memo and let visitors compare the public-chain view with the user’s view. Explain that a viewing key is read-only but discloses transaction data, so it is a sensitive credential and should not be presented as a casual share link.

### Cited Findings
- Zcash Foundation explains that a viewing key decrypts shielded transaction data but does not reveal the spending key; it gives viewing access without the ability to spend funds. [Zcash Foundation: Introducing Blockchain Scanning with Zebra](https://zfnd.org/introducing-blockchain-scanning-with-zebra/)
- Zcash’s glossary describes viewing keys as allowing access to the balance and transaction data of an associated shielded address, and calls this form of sharing “selective disclosure.” [Zcash Glossary](https://zcash.readthedocs.io/en/latest/rtd_pages/glossary.html)
- The Zcash wallet UX checklist recommends viewing keys for watch-only wallets, encourages secure communication channels, and says memo fields should remain visible in the transaction UI even when empty. The page is versioned 6.12.2 and some adjacent details are explicitly tied to older wallet versions, so use it as UX guidance rather than as a complete current UFVK specification. [Zcash Feature UX Checklist](https://zcash.readthedocs.io/en/latest/rtd_pages/ux_wallet_checklist.html)
- ZIP 311 is a **draft** standard for payment disclosures. It notes that the blockchain alone does not contain enough information to reconstruct and verify many shielded payment details; its UI section still has a TODO on how validity states should be displayed. [ZIP 311: Zcash Payment Disclosures](https://zips.z.cash/zip-0311)
- The repo’s design says `/viewkey` returns the user’s UFVK, while the threat model says the operator holds all user seeds and can see the orders and phone numbers. [Repo detailed design](../../docs/05-detailed-design.md) · [Repo threat model](../../docs/07-threat-model.md)

### Inferences
- Label the control **“View as: Public chain / You (viewing key)”** or similar, and use the same sample receipt fields in both views. For the user view, reveal the memo; for the chain view, show that the memo and amount are encrypted. Don’t present a txid alone as proof of the full shielded payment; ZIP 311 says additional disclosure information is needed for some cases. [Zcash: Addresses and Value Pools](https://zcash.readthedocs.io/en/latest/rtd_pages/addresses.html) · [ZIP 311](https://zips.z.cash/zip-0311)
- Put a short warning next to `/viewkey`: **“Read-only, but it reveals this account’s transaction data. Share it only with someone you trust.”** A private Telegram chat protects against group disclosure, while the repo’s threat model says the operator holds all seeds; the page should not imply the user has exclusive access. [Zcash Foundation: Viewing Keys](https://zfnd.org/introducing-blockchain-scanning-with-zebra/) · [Repo threat model](../../docs/07-threat-model.md)
- Show the receipt memo as real product evidence, not decorative ciphertext: include an order reference, operator, rupee amount, status, and the explanation that only an authorized viewer can decode the shielded transaction data. Confirm the exact fields against the implementation before publishing them. [Zcash Glossary](https://zcash.readthedocs.io/en/latest/rtd_pages/glossary.html) · repo: `docs/05-detailed-design.md` §3.4

### Gaps
- Public sources do not verify exactly what this repo’s zcash-devtool UFVK reveals for an account under the current testnet/Ironwood toolchain. The repository’s planned `list-tx` and memo behavior still has an open Phase 0 check in `docs/05-detailed-design.md` §3.5.
- ZIP 311 is draft, and its own UI-validity guidance is unfinished; don’t promise independent, cryptographic public verification of a receipt based on this proposal.

## Which privacy-page patterns are useful for this project?

### Takeaway
Use Zcash’s simple protocol vocabulary and Zodl’s benefit-plus-example structure, but keep a visible boundary between on-chain privacy and service-level privacy. Avoid importing the “private by default” or “no tracking” claims of products whose architecture differs from SSA.

### Cited Findings
- Zcash’s public site groups educational material under “Zcash 101,” “Privacy,” and “Using Zcash,” then links related topics such as unified addresses, wallet selection, and spending. It provides a path from a short answer to deeper reading. [Zcash: Is Zcash traceable?](https://z.cash/learn/is-zcash-traceable/) · [Zcash: What are unified addresses?](https://z.cash/learn/what-are-zcash-unified-addresses/)
- Zodl uses a large app image and one download CTA in its first screen, then repeats download actions after feature groupings. Its site separates protocol privacy, self-custody, real-world purchases, tracking, shielding, and notes as distinct explanations. [Zodl](https://zodl.com/)
- Signal’s support page puts the privacy model, verification mechanism, source-code link, and a “Have more questions?” support action together on one short page. Its architecture is not comparable to a Telegram bot conversation, which the SSA project already identifies as visible to Telegram and the operator. [Signal: Is it private? Can I trust it?](https://support.signal.org/hc/en-us/articles/360007320391-Is-it-private-Can-I-trust-it) · repo: `docs/05-detailed-design.md` §10.2, §10.5
- The repository’s FR-15 requires one informational landing page, a Telegram CTA and bot QR, judge try-it steps, limits, repository link, mobile readability, and no trackers or third-party requests. FR-16 adds aggregate-only live stats with explicit offline behavior. [Repo SRS](../../docs/03-srs.md)

### Inferences
- Use the page sequence **what it does → one scripted Telegram quote/confirm/done flow → how the shielded receipt differs from a public transaction → who can see which data → caps and testnet status → try it in Telegram**. Keep the privacy diagram beside the receipt interaction so the claim is supported at the moment visitors see it.
- Keep testnet, operator custody, Telegram visibility, and Bitrefill visibility in short, persistent labels near the CTA or privacy section; don’t bury all four in FAQ copy. This follows the project’s explicit FR-15 scope and threat model, not any external privacy brand’s claims. [Repo SRS](../../docs/03-srs.md) · [Repo threat model](../../docs/07-threat-model.md)

### Gaps
- No current public product screenshots or implementation files for the SSA landing page exist in this checkout, so this research compares public references and the repo specification rather than auditing a finished SSA page.
- The Zcash UX checklist and glossary contain historical/version-specific material; the implementation team should check details against the current wallet and protocol documentation before presenting them as current technical behavior.
