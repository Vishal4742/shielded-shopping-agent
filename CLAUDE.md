# CLAUDE.md — Shielded Shopping Agent

## What this is
A Telegram bot, written in Rust, that buys Indian mobile top-ups (Jio/Airtel/Vi) from a user's shielded Zcash testnet budget via the Bitrefill API. Every purchase is capped by code-enforced limits and leaves a shielded receipt memo readable only with the user's viewing key. Built for the Free Society Hackathon (Mumbai). **Project source code is written only from 4 Nov 2026 11:00 IST** (unless the hosts approve earlier in writing). Submission deadline 4 Nov 23:45 IST.

- Design and decisions: `docs/00-README.md` (charter, SRS, architecture, detailed design, ADRs, threat model, test plan, plan, risks, traceability). Change the SRS first, then traceability, then design and tests.
- Build log: `notes/` (`phase-N.md` per phase; `devtool.md`, `stack.md` = Phase 0 findings)

## Commands
- Environment: Rust 1.99 (`cargo`); zcash-devtool binary at `tools/zcash-devtool/target/release/zcash-devtool` (commit `0b3436b`); testnet wallet `tools/wallets/ssa`
- Run: `cargo run --release` (reads `.env`)
- Tests: `cargo test` · lint/format: `cargo fmt && cargo clippy -- -D warnings` (clippy gate only in Phase 7) · types: `cargo check`
- Smoke: `/smoke` (detects the stack and runs everything above)

## Conventions
- Rust stable; tokio, teloxide, reqwest, rusqlite (versions in `notes/stack.md`). Money is integers only (zat, paise, cents).
- Pure modules (`caps`, `quote`, `order_fsm`, `zip321`, parsers) have no I/O and take the clock as a parameter; write their tests first.
- Every devtool call goes through `zec` under the wallet mutex. Every order state change goes through `order_fsm` plus a conditional SQL update.
- The LLM only extracts intent; it never triggers a purchase. Limits are enforced in code (SRS BR-1..5).
- Secrets only in `.env` (gitignored); `.env.example` lists every key; never log secrets, UFVKs, seeds, redemption codes or full phone numbers.
- Every network call goes through one module per service (`bitrefill`, `intent`, `price`, `zec`).
- Commits: `<type>: <description>` with type in feat, fix, refactor, docs, test, chore; reference the requirement ID (e.g. `FR-8`).

## Workflow (Claude Code team tips + ECC loop + Ponytail)
- **Ponytail ladder before code:** does it need to exist → already in this repo → stdlib → platform → installed dependency → one line → only then the minimum that works. Read the code the change touches first. Never cut validation, error handling, security or anything requested. Deliberate shortcuts get a `ponytail:` comment naming the ceiling and upgrade trigger.
- **Plan:** complex task → plan mode; `plan-reviewer` agent reviews the plan as a staff engineer; re-plan when something goes sideways.
- **Build:** research/reuse first (`search-first`); tests first for pure logic (RED → GREEN commits); fixtures + `/smoke` for model/network code; then `/code-review` from a fresh context, `verification-loop`, and the `verify-app` agent.
- **Commit:** `/ponytail-review` on the diff; "Grill me on these changes and don't commit until I pass your test"; "Prove to me this works" with real output; `/commit-phase`.
- **Phase close:** `/phase-close` (review, verification loop, `/ponytail-audit`, notes, prune this file). Independent phases run in git worktrees under `.claude/worktrees/`.
- **Publish:** `/security-scan` and `npx -y ecc-agentshield scan --path .` first.

## Mistakes and rules
Append one line after every correction ("Update your CLAUDE.md so you don't make that mistake again"). Prune at the end of each day.

- Follow standard practice before any building: requirements → design → plan → review, then code. Don't jump to builds or code without the user's go-ahead.
- Use Rust wherever possible (user preference).
- Before 4 Nov, Phase 0 work is shell transcripts, notes and docs only. No project Rust code.
