# R: Stored program bytes and contract instances — evidence

## Program bytes (`wasm_programs.code`, 2026-10-06)

Local run of `wasm-code-backfill` on 46 programs drawn from production (40
random with an interface + all 6 without one): 46/46 fetched, sha256 46/46,
byte length = `wasm_byte_len` 40/40, metadata identical to production 40/40,
the 6 spec-less rows have empty metadata; a second run found nothing missing.
Production holds no row without the `upgradeable` key (0 of 5,238), so the
backfill's recomputed metadata changes nothing on that account.

## Contract instances (`contract_instances`, 2026-10-07)

- Size, sample of 300 random contracts via `getLedgerEntries`: p50 392 B, p99
  564 B, max 2,364 B, mean 375 B — ~57 MB raw for ~152k contracts. 149/300 are
  archived on the ledger; the RPC returned all 300.
- Local `contract-instance-backfill` (300 random contracts + 20 SAC): 320/320
  written; bytes and ledger equal to `getLedgerEntries` 320/320; a re-run
  finds nothing missing.
- Live path, 4 committed fixture ledgers: the extractor and the JSON change
  path name the same 187 instance changes in the same order, each decoding
  back to an instance of the contract named.
- No TTL is stored. `invoke_host_function_in_recording_mode`
  (soroban-env-host 29) requires a `live_until` for every contract entry
  (`None` fails the call), so the executor passes one at or past the current
  ledger: archived instances read as live, which is right for read-only view
  calls (an earlier value would trigger a restore).
- Persistent entries a few token functions read (17 contracts on 10
  programs, re-measured 2026-10-07) are not stored; where they come from is
  deferred to task 0633.

## Local executor and metadata backfill (2026-10-07)

Local ClickHouse with production's `soroban_contracts`, `wasm_programs`
metadata, `soroban_contract_metadata` and latest ledger; bytes (5,262 of
5,263 programs) and instances (156,312 of 156,312) filled from RPC.
`contract-metadata-backfill` over the 4,360 non-SAC contracts whose program
declares `decimals` (26 s):

| Outcome                                           | Contracts |
| ------------------------------------------------- | --------- |
| equal to the stored row                           | 3,864     |
| differs — stored `decimals` NULL, function says 0 | 2         |
| no stored row before                              | 347       |
| needs persistent contract data (task 0633)        | 45        |
| function fails (also fails on RPC)                | 102       |

Against RPC `simulateTransaction` on one contract per token program (330):
299 equal on all three values, 0 different; 23 fail on RPC too; the other 8
are task 0633 contracts. The three tokens behind the four Aquarius pools of
0617 now have decimals: XRP 6, HITZ 7, USST 18.

Trap found: `contract IN (SELECT unhex(…))` against a `FixedString(32)`
column drops a trailing zero byte and misses every id ending in `0x00` (22
contracts here); `toFixedString(…, 32)` in the subquery fixes it. Plain
`= unhex(?)` is not affected.
