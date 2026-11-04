# Landing page research (31 Oct 2026)

Three research passes (web + local ETHGlobal winners file). Applied in docs/05 §10.2 and §10.5.

## Hackathon winners
- Evidence is thin: `~/ethglobal/research/ethglobal_winners_11_events.json` (1557 records) has no site URL field; 7 winner sites fetched as text only.
- Pattern: one-line value prop + one action, and **show the product working** (ENShell `npm install` hero, enshell.xyz; Frames curl snippet; Void Tactics stat card). Several winners had minimal or no sites (Capitola, SP3ND with only Loom videos): judges reward the demo and the video, not polish.
- Credit lines naming sponsor tech signal track fit ("Built with ENS · Chainlink · Ledger").
- Avoid: placeholder text left live ("Loading ship..."), copy errors, long explanations before the demo.
- Sources: blog.colosseum.com/announcing-the-winners-of-the-solana-cypherpunk-hackathon · zechub.substack.com/p/zechub-hackathon-results-are-in · enshell.xyz · voidtactics.xyz · zyberquest.vercel.app

## Hero and motion
- Budget: one hero entrance (600 to 900 ms), at most one ambient loop, everything else once on scroll; transform and opacity only.
- Spring easing with CSS `linear()` plus a cubic-bezier fallback (developer.chrome.com/docs/modern-web-guidance/guides/ui-behaviors/physics-based-easing, joshwcomeau.com/animation/linear-timing-function).
- Scroll-driven animations (`animation-timeline: view()`): Chromium yes; Safari and Firefox support conflicting in sources, so gate with `@supports` or keep IntersectionObserver.
- Cheap tells: gradient headline text, identical fade-up on everything, parallax on text, loops running off screen, aurora blobs.
- Note: this pass ran only 3 searches; Awwwards, Godly and YouTube were not opened.

## Privacy storytelling
- "X sees A, not B" headlines (apple.com/privacy); short negatives (signal.org); limits next to claims (mullvad.net "a VPN is not enough"; mercury.com "not a bank"; aztec.network alpha label).
- Live counters elsewhere showed placeholders or inconsistent numbers (z.cash, penumbra.exchange, aztec.network): one data source, "n/a" on failure.
- Zashi is now Zodl (zodl.com) and claims no tracking while its site runs analytics: keep the page tracker-free.
- Zcash brand: logos at z.cash/press; rules at zfnd.org/zcash-trademark-policy (compatibility wording allowed, logo not more prominent than ours, link to z.cash, no implied endorsement, no Zcash in our name).
- Bitrefill (bitrefill.com) makes no no-KYC claim and now accepts cards: don't claim either for it.

## Tried on the test page (fictional "Tiffin Pass")
Hero entrance with spring easing · hero app window demo loop (paused off screen) · stats count up once · "One lunch, three views" toggle with decode transition · "What we can't hide" strip · Blender scan reveal · riso scene.

## Graphify (graphify.com), user pick
- Hero: mono headline on black highlight bars that types/decodes in with an RGB-split glitch; dashed wireframe 3D cube; "COPY SETUP PROMPT" box; "works with" logo row; stats in corner crop marks that count up; announcement bar with version chip
- Sections: One map of the code · Answers with proof · See what breaks · feature grid · "Graph, chunks or grep." comparison · "Run it on your machine, or let us run it." (On-device · No telemetry · Apache 2.0 · Yours to host) · FAQ · final CTA
- Taken: decrypting headline (fits "encrypted → readable"), copy chip, crop marks, trust row, banner chip. Skipped: floating math glyphs (clutter on top of riso grain)
