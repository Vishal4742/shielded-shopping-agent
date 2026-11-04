# Contributing

1. Branch off `main` and open a pull request. CI must be green: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`. Changes to `web/` get a Vercel preview link on the PR.
2. Commit messages: `<type>: <description> (FR-x)` with type `feat`, `fix`, `refactor`, `docs`, `test`, `chore` or `ci`.
3. Money is integers only (zat, rupees, cents, satoshis); no floats.
4. Pure modules (`caps`, `quote`, `order_fsm`, `zip321`, the parsers) take the clock as a parameter and get tests first.
5. Every order state change goes through `order_fsm` plus a conditional SQL update; every devtool call goes through `zec` under the wallet lock.
6. The LLM only reads intent; it never triggers a purchase. Limits stay in code.
7. Never log or commit secrets, viewing keys, seeds, redemption codes or full phone numbers. Keys go in `.env` only (see `.env.example`).

Before writing new code: does it need to exist, is it already here, does the standard library or an installed crate do it? Mark deliberate shortcuts with a `ponytail:` comment naming the limit and when to upgrade.
