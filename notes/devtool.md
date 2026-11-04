# zcash-devtool findings (P0.3–P0.5), 2026-10-30

Binary: `tools/zcash-devtool/target/release/zcash-devtool` (commit `0b3436b`, release build 1m42s, 95 MB).
Testnet server: default `-s zecrocks` resolves to `https://testnet.zec.rocks:443` when the wallet network is `test`.

## Wallet created
- Dir: `tools/wallets/ssa` (gitignored) · age identity: `tools/wallets/identity.age` (gitignored)
- Account 0 `treasury` uuid `093fb250-5e57-4ca4-a371-ce280b15b7be`, birthday 4557152
  - UA: `utest10mevf5tzcq9zstmdxysn0yk3s2t3e5x9t36wujmdah2prcenj2n77498lqkymru7h6lyhw4ss8apdz8dchpzgvuzzef3ssdd9krhwdd65qany8rt9xv7nnphnet5t70kt8x8whpqmrtlv6u2mll7q554lgwwn2k2r687kzya0t9gxyjkud2s9htprl47yehz5ezzllv7v6mlgh8xgym`
- Account 1 `user-test` uuid `d87ee4a6-ea65-4258-b903-713b7ba04ede`
  - UA: `utest1g7jsecf3d384rle5z7yj70dhlfjzws24nc6863thqzxn5d7ama8x4j3kgqjmef7t979k8raccls4khzm27tr9gj8m98y672lv5ykgvv6tjazhvvug9am3q4v00usu94rt86ef23kqu6tqeas7ayus65plujqp2a4jpfzz357f5ux7jfxugus392cxexcda00t4ekaxdxsql46jjyysf`

## Answers
| Question | Answer |
|---|---|
| **Q2 multiple accounts per wallet** | **Yes.** `wallet generate-account -i <identity> --name <n>` → new ZIP-32 account index; each has its own UFVK in `list-accounts`. → design change: ONE wallet, one account per user (ADR-003 revised) |
| Account-scoped commands | `balance [ACCOUNT_ID]`, `list-tx [ACCOUNT_ID]`, `list-addresses [ACCOUNT_ID]`, `send ... [ACCOUNT_ID]` take the account **UUID** |
| Machine-readable balance | `balance [uuid] --json` → `{"chain_tip_height","ironwood_spendable","orchard_spendable","sapling_spendable","total","transparent_spendable"}` (zat). Fails before the first sync |
| Confirmations | `balance`/`send` accept `--min-confirmations N` (default 3 trusted / 10 untrusted) |
| Send | `send -i <identity> --address <ua> --value <zat> --memo <text> [uuid]` |
| Tx list | `list-tx --json` → `[{"txid","mined_height"}]` only. `list-tx -m csv` columns: `Date,Action,Symbol,Volume,Currency,Account,Total,Price,Fee,FeeCurrency,Memo` (**no txid column**) |
| Server info | `get-info` → `{"chain_name":"test","chain_tip_height":4557252,"server_uri":"https://testnet.zec.rocks:443"}` |
| Sync time | ~2 s for 100 blocks from a fresh birthday |
| UFVK display | `list-accounts` prints UIVK + UFVK for every account. The app must never log this output |

## Faucet receive (2026-10-30, fauzec.com)
- treasury tx `3a999d11…8a90` mined 4557289; user-test tx `75275115…54ca` mined 4557290; 1 TAZ each
- Notes land in the **Ironwood** pool (NU7), not Orchard → read `ironwood_spendable` from `balance --json`
- At tip 4557296 (7 confs): `total` 100000000, all `*_spendable` 0. External receipts are untrusted, so the default needs 10 confirmations (~4 min at 25 s blocks)
- `balance --min-confirmations 0` is rejected (CLI error); the minimum is ≥ 1
- `list-tx` text mode shows txid, mined height, time, amount, pool, receiving account name+uuid, "N memos"; "Fee paid: Unknown" for received txs

## Open items (need funded wallet)
- [ ] Receive with a memo → does text mode show the memo next to the txid? (the faucet drips had no memo)
- [ ] Account 1 → treasury send with `--memo`; check that the memo appears for both accounts
- [ ] `init-fvk` / `import-ufvk` watch-only wallet sees account 1's memos and nothing from account 0
- [ ] Fee actually charged for a small shielded send (ZIP-317)
- [ ] Time from send → visible after sync (25 s blocks)
