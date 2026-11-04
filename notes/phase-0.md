# Phase 0: Prep (30 Oct – 3 Nov 2026)

Plan: `docs/09-project-plan.md` §3. No project source code in this phase.

## Done
| Task | Result | Evidence |
|---|---|---|
| Docs baseline v1.0 | 14 docs + design review (15 findings applied) | `docs/00-README.md` |
| P0.2 harness | three-techniques (rust pack): ECC + Ponytail, agents, commands, hooks | `.claude/`, `CLAUDE.md` |
| P0.3 zcash-devtool | Release build OK (1m42s) at commit `0b3436b` | `tools/zcash-devtool/target/release/zcash-devtool` |
| P0.4 wallet | Testnet wallet `tools/wallets/ssa`: acct 0 treasury, acct 1 user-test | `notes/devtool.md` |
| P0.5 (part) | CLI surface verified; Q2 answered (multi-account works) → ADR-003 revised | `notes/devtool.md` |
| P0.9 crates | Versions recorded | `notes/stack.md` |

## Open
- [ ] P0.1 Ask hosts: pre-event code allowed? WebZjs after Ironwood? (user)
- [ ] P0.4 Collect TAZ daily into the treasury UA (user)
- [ ] P0.5 Funded tests: receive, memo send, list-tx parsing (txid + memo), UFVK watch-only, fee, latency
- [ ] P0.6 Bitrefill key + $20 + manual ₹10 Jio; product JSON fixtures; Q5 reference field (user + Claude)
- [ ] P0.7 Anthropic key; one real curl with the planned body (Claude)
- [ ] P0.8 Telegram bot token (user)
- [ ] P0.10 Zingo testnet on phone; judge wallets pre-funded the night before
- [ ] P0.11 Demo script, slides, invite codes
