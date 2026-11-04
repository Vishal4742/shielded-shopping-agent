# 13 — Engineering Standards and Definition of Done

## 1. Code standards (Rust)
- `cargo fmt` formatting; `cargo clippy` clean before release (Phase 7).
- Money is integers only (`u64` zat, `u32` paise/cents); `u128` for intermediate maths. No floats.
- Errors: `thiserror` in modules, `anyhow` only in `main`. No `unwrap()`/`expect()` outside tests and startup config.
- Pure logic (`caps`, `quote`, `order_fsm`, `zip321`, parsers) has no I/O and takes the clock as a parameter.
- Every devtool call goes through `zec`, which takes the wallet lock. Nothing else spawns devtool.
- Every order state change goes through `order_fsm` plus a conditional SQL update.
- Deliberate shortcuts get a `ponytail:` comment naming the limit and when to upgrade.
- Comments explain *why*, not *what*. Keep them short.

## 2. Security standards
- Never log secrets, UFVKs, seeds, redemption codes or full phone numbers.
- Read secrets from env only; `.env` and `data/` are gitignored.
- Treat all text from users, the LLM and Bitrefill as untrusted data.

## 3. Git conventions
- Branch: `main` (hack day, small team). A commit at every green phase gate.
- Conventional commits: `feat:`, `fix:`, `test:`, `docs:`, `chore:`.
- Secret grep before every commit (07 §5).

## 4. Definition of Done (per feature)
- [ ] Requirement ID referenced in the commit message
- [ ] Unit tests for pure logic pass (written first)
- [ ] Phase-gate manual check passes
- [ ] Errors give a catalogue message (05 §9)
- [ ] No secrets or PII in logs
- [ ] Traceability matrix still accurate

## 5. Definition of Done (release)
- [ ] All M test cases pass, or are listed in the README as known issues
- [ ] Security checklist (07 §5) complete
- [ ] README: what it is, how to run, architecture diagram, **disclosure of all pre-event prep**, roadmap
- [ ] Demo video recorded; submission sent before 23:45
