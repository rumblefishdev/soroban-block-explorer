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
- Persistent entries a few token functions read (17 in the spike) are not
  stored; the executor fetches them from RPC.
