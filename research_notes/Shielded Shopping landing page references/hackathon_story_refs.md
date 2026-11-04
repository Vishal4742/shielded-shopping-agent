# Hackathon and privacy product storytelling

## How does a first-time visitor understand the product quickly?

### Takeaway
The best fit for SSA is a single concrete purchase story with a plain-language value proposition and one immediate Telegram action. Keep the page informational: it should show what the bot does, then hand the visitor to Telegram rather than imitate a web checkout.

### Cited Findings
- Zcash's current homepage leads with “Zcash is encrypted electronic cash,” then offers distinct “Use Zcash” and “Learn Zcash” actions; its feature explanations are short and task-oriented. [Zcash homepage](https://z.cash/)
- Bitrefill explains its customer flow as three named steps: pick a product, pay with crypto, receive it. It also places phone refills alongside gift cards and eSIMs in the main product navigation. [Bitrefill homepage](https://www.bitrefill.com/)
- A ZecHub hackathon results page gives each winning project a compact description and an embedded demo video; the examples include a Zcash-powered bounty platform, Zcash donation app, and privacy-focused wallet/chat prototype. [ZecHub Hackathon Results](https://zechub.substack.com/p/zechub-hackathon-results-are-in)

### Inferences
- Adapt this to one hero sentence: “Buy a Jio, Airtel or Vi top-up from a shielded Zcash budget in Telegram.” Keep “Open in Telegram” as the primary action and a short demo as the secondary action.
- A three-step strip is enough for the first explanation: fund a shielded budget → review and confirm a quote → receive the top-up and private receipt. Put the QR and judge try-it steps after the core flow.
- Keep the testnet and invite-only status visible near the CTA so visitors understand the demo constraints before trying it.

### Gaps
- Zcash and Bitrefill pages are commercial/official sites, not hackathon landing pages. ZecHub's results page is a project showcase rather than a single product landing page.
- The web fetcher exposed page structure and copy, not pixel-accurate screenshots or mobile breakpoints.

## How can the page prove the privacy model without overclaiming?

### Takeaway
A concrete “who sees what” interaction is stronger than a generic privacy promise. The strongest references mark demo data and state the exact limits next to the proof.

### Cited Findings
- Privacycoins.io contrasts clickable public Bitcoin payments with shielded Zcash payment stubs and a simulated viewing-key interaction. It says “Demo only — not a real decrypt” and distinguishes live Bitcoin rows from Zcash proof stubs. [Shielded Zcash explainer](https://privacycoins.io/)
- ZK Freighter's project README presents a 30-second demo link, product and source links, and explains which events remain public (shield-in, unshield-out, bridge arrivals). It labels the software unaudited, recommends testnet, warns against real funds, and rejects “anonymous” / “untraceable” language. [ZK Freighter project README](https://github.com/Blockchain-Oracle/zk-freighter)
- Zcash's official explainer separates shielded from transparent activity and says shielded transfers hide addresses, amounts, and memo fields from public view, while transparent activity is publicly visible. [Shielded vs. transparent Zcash](https://z.cash/learn/what-is-the-difference-between-shielded-and-transparent-zcash/)
- ETHGlobal's Hidden Garden showcase pairs a short product description with “Live Demo” and “Source Code” links, then describes selective disclosure in plain terms. [Hidden Garden showcase](https://ethglobal.com/showcase/hidden-garden-iek02)

### Inferences
- SSA can show one receipt in three views: public chain, Bitrefill, and the user with a viewing key. Label the fields each party can see and distinguish simulated illustrations from live data.
- The claims should name the boundary: shielded receipt details are readable with the user's viewing key; Bitrefill still receives the phone number and top-up amount; Telegram still handles the chat; the operator holds the demo wallet keys. Avoid “leave no trail,” “anonymous,” or statements implying the operator cannot see wallet activity.
- Link the repository and demo evidence near the page footer or judge section, like the hackathon showcase references.

### Gaps
- The ZK Freighter live product site could not be fetched, so notes use its first-party GitHub README rather than a visual inspection of the site.
- Privacycoins.io is an independent explainer, not an official Zcash or ECC property. Its demo is explicitly simulated and should be treated as a storytelling reference only.

## How should stats, testnet status, and judge steps appear?

### Takeaway
Small, source-labeled metrics can establish that the demo is running, but every number needs an “as of” time and an honest unavailable state. A public page should make the judge path easy without making the page itself a purchase surface.

### Cited Findings
- Zcash's homepage pairs its core explanation with network metrics such as price, outstanding supply, and shielded ZEC. The site also links into separate learn, ecosystem, and build sections. [Zcash homepage](https://z.cash/)
- The Chainlink Convergence hackathon gallery explicitly labels entries as participant descriptions that are not independently verified or audited, then groups submissions by track and award. [Convergence project gallery](https://chain.link/hack-26)
- ZecHub's hackathon results page uses embedded project videos and links to demos and source repositories, allowing readers to move from summary to proof without treating every entry as production software. [ZecHub Hackathon Results](https://zechub.substack.com/p/zechub-hackathon-results-are-in)
- Bitrefill's current homepage presents a product catalogue and a short “How it works” flow, while its phone-top-up documentation makes clear that a top-up is applied to the supplied phone number rather than returning a code. [Bitrefill homepage](https://www.bitrefill.com/), [Bitrefill phone top-ups](https://docs.bitrefill.com/docs/phone-topups)

### Inferences
- Follow the repo's FR-16 schema only: delivered top-ups today, rupees committed vs. the daily cap, and total shielded receipts. Show the update time, testnet label, paused/offline states, and no per-user or per-order detail.
- Make the judge steps a small numbered path: get an invite at the table → scan/open the Telegram bot → `/start <code>` → use the pre-funded test wallet or deposit. Do not put a public invite code on the landing page.
- Treat “live stats” as proof only when they come from the bot's aggregate endpoint; mark sample/demo figures as sample data. The Chainlink gallery's explicit unaudited note is a useful transparency pattern for hackathon work.

### Gaps
- The actual Telegram bot is invite-only and was not inspected in this research task, so the landing page can only describe the documented path from the repo's SRS.
- Reference pages do not validate SSA's live numbers, privacy behavior, integration state, or deployment claims.
