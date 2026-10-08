---
id: '0620'
title: 'Execute contract view functions locally: token decimals, name and symbol from the standard, not from storage keys'
type: FEATURE
status: done
related_adr: ['0061']
related_tasks: ['0617', '0621', '0473', '0340', '0297', '0325']
tags: ['effort-large', 'priority-high', 'soroban', 'tokens', 'liquidity-pools']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: >
      Spawned from 0617. Spike: local decimals() on all 4,207 token contracts
      equals RPC simulation (4,151 same value, 56 same failure, 0 different).
      Decided: store WASM bytes (W248 C), execute locally (W240 A).
  - date: 2026-10-05
    status: active
    who: karolkow
    note: >
      PR split agreed: 1 `wasm_code` + live write + backfill command;
      2 executor + metadata from functions; 3 remove the METADATA reader.
  - date: 2026-10-08
    status: done
    who: karolkow
    note: >
      Shipped in production-2026.10.08-2 (#616-#653; #655 structure follows).
      Production fills: 5,269/5,269 programs with bytes, 156,699/156,699
      instances, 4,320 metadata rows written (367 new; 3,951 equal). TVL of
      the four 0617 pools moved to 0615: their legs have decimals now, but
      Soroban-token legs are never priced.
---

# Execute contract view functions locally

## Summary

Read a Soroban token's `decimals`, `name` and `symbol` by running the
contract's own SEP-41 view functions with the official execution library
(`soroban-env-host`), inside our pipeline, instead of guessing the storage key
the author chose. Store each WASM program's bytes once, at upload. Decision
recorded in [ADR 0061](../../../2-adrs/0061_execute-contract-view-functions-locally.md).

## Context

SEP-41 fixes the functions, not the storage layout. The parser reads only
`Symbol("METADATA")` / `Vec[Metadata]` (`xdr-parser/src/token_metadata.rs`).
Census of the 4,207 non-SAC contracts whose interface exports `decimals` and
`balance` (2026-10-05):

| Where decimals live                                                                                 | Contracts     | Read today |
| --------------------------------------------------------------------------------------------------- | ------------- | ---------- |
| `METADATA` → `{decimals}` / `{decimal}`                                                             | 2,459 / 1,391 | yes        |
| `Vec[Metadata]` → `{decimal(s)}`                                                                    | 18            | yes        |
| constant in code                                                                                    | 79            | no         |
| `Vec[Meta]`, `Vec[State]`, `Vec[Config]`, `Vec[Decimals]`, `Symbol("Metadata")`, `STORAGE`, 6 rarer | 212           | no         |
| contract never initialised / traps                                                                  | 48            | —          |

Guessing by key name is unsafe: in vaults a key named `*Decimals` holds the
underlying asset's scale — `VirtualDecimalsOffset` 12/12, `UnderlyingDecimals`
5/7, `AssetDecimals` 1/1, `CalibratedDecimals` 1/1 differ from `decimals()`.

Stored values today are correct where present: 3,864 equal `decimals()`,
0 differ; 4 have a stored value while the contract now fails (`#105`).
287 lack a value the contract returns — among them the three tokens that
leave four Aquarius pools without TVL (0617).

## Spike results (2026-10-05)

Code: `spike/main.rs.txt`, `spike/Cargo.toml.txt` (renamed so the workspace
does not build them).

- `soroban-env-host =29.0.0`, feature `recording_mode`,
  `e2e_invoke::invoke_host_function_in_recording_mode`, snapshot source = a
  map of ledger entries; a key the run asks for and the map lacks is fetched
  and the call re-run (25 of 4,207 needed one re-run).
- Budget from the network's own config settings (`ContractComputeV0`: 400 M
  instructions, 40 MiB); the default `Budget` (100 M) is too small.
  `LedgerInfo` needs the real protocol (mainnet is on **29**) and
  `network_id = sha256(passphrase)`.
- `decimals()` on 4,207 contracts: 4,151 same value as RPC simulation, 56
  same failure, 0 different. `name()`/`symbol()` match on the samples.
- Time per call: p50 0.70 ms, p99 7.6 ms, max 76 ms; 4,207 calls 5.1 s.
- Footprint of `decimals()`: own code + instance 4,045; plus another
  contract's instance 89; plus persistent data 17; failures 56.
- Archived entries execute normally (recording mode marks them for restore).
- `ContractExecutable` has a new `ExternalRef` variant — match it.

## Program bytes (W248)

5,235 distinct programs known (all fetched, 0 missing, sha256 = hash):
112 MB raw, 34–65 MB compressed (0.01% of the database); token programs
10.8 MB raw, 2.3–5.9 MB compressed. Network cap 131,072 B per program.
Growth over the last 6 months ~427 programs/month, ~3–6 MB compressed.
`wasm_interface_metadata.metadata` already carries `wasm_byte_len`. No table
records a program's upload ledger (`soroban_contracts.wasm_uploaded_at_ledger`
is the deploy/upgrade ledger despite its name).

## Rejected at research

See `notes/R-rejected-at-research.md`.

## Implementation

1. Program bytes in `wasm_programs.code` (renamed from
   `wasm_interface_metadata`). The indexer writes the whole row from every
   `ContractCode` change; `wasm-code-backfill` fills known programs from
   `getLedgerEntries`, verified by hash. Production: `RENAME TABLE` (+ view
   for the old name) and `ADD COLUMN code` before the deploy, then deploy,
   drop the view, run the backfill.
2. Contract instances in `contract_instances` (decided 2026-10-07: all
   contracts, raw `LedgerEntryData` XDR, filled from RPC). The indexer writes
   every created, updated or restored instance; `contract-instance-backfill`
   fills the rest, versioned by the entry's last-modified ledger. Production:
   `CREATE TABLE` before the deploy, the backfill after it (mainnet and
   testnet). Evidence for both: `notes/R-stored-bytes-and-instances.md`.
3. `crates/contract-executor` (`soroban-env-host =29.0.0` with its default
   budget — the heaviest call measured 12.5 M of 100 M instructions) and `contract-metadata-backfill`, which
   runs the declared `decimals`/`name`/`symbol` of every token over the two
   tables (PR 3a, split from the live path). Contracts whose functions read persistent data: task 0633.
   Live in the indexer (PR 3b): every token whose instance changed in the
   ledger; checked on 8 token-deploy ledgers, 9/9 equal to RPC.
4. The functions are the only source (PR 4, decided 2026-10-08): the parser
   no longer reads `METADATA`, and NFTs (program declares `name` and `symbol`)
   are run as tokens are. Measured over the 3,955 contracts with a stored row:
   the functions return the same for 3,864 of 3,872 tokens and 77 of 78 NFTs;
   12 contracts (5 needing persistent data, 2 failing, 5 declaring none of the
   three) keep their stored row and get no new write. A ClickHouse read error
   now fails the ledger, as a write error does; the executor no longer catches
   host panics (none in any run).
5. Backfill `soroban_contract_metadata` for every token and NFT contract
   (done on production 2026-10-08, see history).
6. Protocol upgrades: bump `soroban-env-host` with `stellar-xdr`.

## Acceptance Criteria

- [x] Decimals, name and symbol of every token contract come from its
      functions; 0 differences against RPC simulation on one contract per
      program (299/299).
- [ ] The four Aquarius pools of 0617 show TVL — moved to 0615: decimals and
      reserves of both legs are served (local API at the release tag on
      production data), TVL stays null because Soroban-token legs are never
      priced.
- [x] `token_metadata.rs` storage-key reading removed.
- [x] Program bytes stored for every known program; new uploads written live.
- [x] **Docs updated** — `database-schema-overview.md` (`wasm_programs.code`),
      `indexing-pipeline-overview.md`, `xdr-parsing-overview.md`.
