# Shielded Shopping Agent

A Telegram bot, written in Rust, that buys Indian mobile top-ups (Jio, Airtel, Vi) from a user's shielded Zcash testnet budget through the Bitrefill API. Limits are enforced in code, and every purchase leaves a shielded receipt memo that only the user's viewing key can read.

Built for the Free Society Hackathon, Mumbai, 4–5 Nov 2026.

## Getting started

```sh
git clone https://github.com/Vishal4742/shielded-shopping-agent.git
cd shielded-shopping-agent
cp .env.example .env        # ask Vishal for the keys directly, never over a public channel
cargo test
cargo run --release
```

You need Rust 1.99 or newer. Code that talks to the wallet also needs `zcash-devtool` built at commit `0b3436b`:

```sh
git clone https://github.com/zcash/zcash-devtool tools/zcash-devtool
cd tools/zcash-devtool && git checkout 0b3436b && cargo build --release
```

`tools/`, `data/` and `.env` are gitignored. Wallets and keys stay on your machine.

## Where things are

- `src/`: the bot (Rust).
- `web/`: the landing page (static HTML, CSS and JS, no build step). Open `web/index.html` or run `python3 -m http.server -d web`.
- `.env.example`: every config key the bot reads.

Design docs and research notes live with Vishal, not in this repo. Ask if you need the spec for a feature.

## Working together

- Branch off `main`, open a PR, and get a review before merging.
- Commit messages look like `<type>: <description> (FR-x)`, where type is one of feat, fix, refactor, docs, test or chore.
- Money is always integers (zat, paise, cents). Keep secrets in `.env` and nowhere else.
- Run `cargo fmt && cargo clippy -- -D warnings && cargo test` before you push.
