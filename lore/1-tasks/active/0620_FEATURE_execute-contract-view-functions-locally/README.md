---
id: '0620'
title: 'Execute contract view functions locally: token decimals, name and symbol from the standard, not from storage keys'
type: FEATURE
status: active
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

- **Learn a per-program layout descriptor** (which key holds the value):
  reproduces 4,054 of 4,151 contracts, 0 mismatches, but needs the bytes and
  one initialised instance anyway, cannot be learned at upload (285 of 330
  programs trap on an empty instance), breaks on 27 cross-contract programs,
  and adds a second interpreter of storage that goes stale on upgrade.
- **Compute at upload:** only 45 of 330 token programs return decimals
  without state.
- **Cache by (program, storage hash):** 16% hit rate.
- **RPC simulation per contract:** an external dependency with rate limits
  (~50% of calls answered 429 during the census).

## Implementation

1. Program bytes in `wasm_programs.code` (the table renamed from
   `wasm_interface_metadata`; a separate `wasm_code` table was folded in
   before reaching production). The indexer writes the whole row from every
   `ContractCode` change, `Restored` and spec-less programs included; a
   one-off backfill fills the 5,235 known programs from `getLedgerEntries`,
   verified by hash. Production order: `RENAME TABLE` (+ a compatibility
   view for the old name), `ALTER TABLE wasm_programs ADD COLUMN IF NOT
EXISTS code String DEFAULT '' CODEC(ZSTD(3))` — both BEFORE the deploy,
   or the indexer's typed insert fails — then deploy, drop the view, run
   the backfill. The backfill rewrites every existing row with metadata
   recomputed by today's parser. Local run 2026-10-06 (46 programs drawn
   from production: 40 random with an interface + all 6 without one): 46/46
   fetched, sha256 46/46, byte length = `wasm_byte_len` 40/40, metadata
   identical to production 40/40, the 6 spec-less rows have empty metadata;
   a second run found nothing missing. Production holds no row without the
   `upgradeable` key (0 of 5,238), so none changes on that account.
2. Executor module wrapping `soroban-env-host` (pinned to the network's
   protocol): `call_view(contract, fn) -> Result<ScVal, …>` over a snapshot
   source fed by the current ledger's changes, `wasm_programs.code` and the
   contract-instance table (decided 2026-10-06: execution runs in the
   indexer, ADR 0043 — no network round trip).
3. On token deploy, instance change and WASM upgrade: run `decimals`, `name`,
   `symbol`; write `soroban_contract_metadata` with the ledger. Remove the
   `METADATA` storage read once the backfilled values match.
4. Backfill `soroban_contract_metadata` for every token contract.
5. Protocol upgrades: bump `soroban-env-host` with `stellar-xdr`.

## Acceptance Criteria

- [ ] Decimals, name and symbol of every token contract come from its
      functions; 0 differences against RPC simulation on one contract per
      program.
- [ ] The four Aquarius pools of 0617 show TVL.
- [ ] `token_metadata.rs` storage-key reading removed.
- [ ] Program bytes stored for every known program; new uploads written live.
- [ ] **Docs updated** — `database-schema-overview.md` (`wasm_programs.code`),
      `indexing-pipeline-overview.md`, `xdr-parsing-overview.md`.
