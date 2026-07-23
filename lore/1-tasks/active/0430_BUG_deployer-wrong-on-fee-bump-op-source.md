---
id: '0430'
title: 'BUG: deployer_id stores the inner-tx source instead of the op source on fee-bump envelopes'
type: BUG
status: active
related_adr: []
related_tasks: ['0255', '0256', '0252']
tags:
  [
    priority-high,
    effort-medium,
    layer-xdr-parsing,
    layer-indexer,
    data-integrity,
  ]
links:
  - crates/xdr-parser/src/op_source.rs
history:
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0256 (Phase 3 validation of the 0255 fix arc). 0255 corrected
      deployer attribution from tx-source to op-source in 2026-05 and 0256 was
      meant to confirm it held. It does not hold on fee-bump envelopes —
      demonstrated on raw XDR from Soroban RPC, decoded with the official
      `stellar` CLI 26.0.0 rather than our own parser.
      Note for whoever validates the fix: Horizon is NOT a sufficient oracle
      here. Its `source_account` for a fee-bump transaction is the INNER
      transaction source — the same value we store — so any comparison against it
      passes trivially. That is exactly how 0256's first pass produced a
      meaningless "30/30 match".
  - date: '2026-07-23'
    status: active
    who: karolkow
    note: >
      Activated. Also answered the question "why is the pre-fix data correct while
      everything after is wrong" — see the new section "The 0255 backfill was a
      one-off SQL swap, and the boundary is a landmine". Short version: the correct
      1,565 rows come from a hand-run `EXCHANGE TABLES` migration, not from the
      parser; they survive only because **no full reindex has re-parsed their
      ledger range since**, proven from the data (each has exactly one raw row —
      a reindex would have inserted a second, wrong one). Fixing the parser is now
      also a prerequisite for ever running a full historical reindex safely.
---

# BUG: `deployer_id` is the inner-tx source on fee-bump envelopes

## Summary

For contracts deployed through a **fee-bump transaction whose operation carries
its own `source_account`**, we store the _inner transaction_ source as
`soroban_contracts.deployer_id`. The correct value — per the protocol docs and
the definition recorded in 0256 — is the **effective operation source**, i.e.
`operation.source_account` when present.

Users see the wrong account credited as the deployer on contract detail pages.

## Evidence (raw XDR, official tooling, not our parser)

Both envelopes fetched with `getTransaction` on Soroban RPC and decoded with
`stellar xdr decode --type TransactionEnvelope`:

| contract        | outer (fee) | inner tx source — **what we store** | `op.source_account` — **correct** |
| --------------- | ----------- | ----------------------------------- | --------------------------------- |
| `CDNEY3YNWS57…` | `GA74RB6L…` | `GB7CY43V…`                         | `GCNP4JVZ…`                       |
| `CC3Y5UFEJS3L…` | `GA74RB6L…` | `GAHQZMZJ…`                         | `GCNP4JVZ…`                       |

The auth entry on the first one carries `credentials: "source_account"`, so the
authorizer resolves to the effective op source — `GCNP4JVZ…` — not the inner tx
source.

Independent cross-check: **stellar.expert returns `GCNP4JVZ…` as `creator` for
6 of 6** freshly-deployed contracts sampled. They read `op.source_account`.

## Root cause — located, and it is NOT the fee-bump nesting

Traced through the code after the XDR evidence. The extraction function itself
is **correct**:

```rust
// crates/xdr-parser/src/op_source.rs
let effective_source = op.source_account.as_ref()
    .map(muxed_to_g_strkey)
    .unwrap_or_else(|| tx_source.to_string());
```

That is exactly the spec rule. The defect is _which contracts ever reach it_.

`extract_op_source_per_contract` only records a contract when it finds an
explicit creation:

- a top-level `CreateContract` / `CreateContractV2` operation, or
- a `CreateContractHostFn` / `CreateContractV2HostFn` node in the auth tree.

Everything else falls through — literally:

```rust
SorobanAuthorizedFunction::ContractFn(_) => {}   // op_source.rs:168
```

**In the observed transactions the factory creates the contract inside its own
code.** The auth tree contains only `ContractFn { create_collateral }`; there is
no `CreateContractHostFn` node anywhere. So the contract is never added to
`deployer_by_contract`, and `extract_contract_deployments` falls back to the
plain `tx_source` it was handed — which for a fee-bump envelope is the inner
transaction source.

Fee-bump is therefore a **red herring**: it makes the wrong value more visibly
wrong (three candidate accounts instead of two), but the bug would fire on a
plain envelope too, whenever a contract is born inside a contract call rather
than through a declared `CreateContract`.

Consequence for the fix: **`tx_auths::auths()` alone will not solve this.** The
contract leaves no trace in the auth tree to walk. The deployment is only
visible in the ledger-entry changes — which `extract_contract_deployments`
already consumes. The fix is to hand _that_ function the effective operation
source instead of the bare `tx_source`. Soroban permits one operation per
transaction, so "the operation" is unambiguous.

## Scope — and the timeline says the live parser never fixed this

**The 1,565 contracts that DO carry `GCNP4JVZ…` are all pre-fix.** Their
`deployed_at_ledger` range is 61,624,053 – 62,527,999, i.e. everything up to
**2026-05-12**. The 0255 fix landed **2026-05-22**.

|                                                  | count     |
| ------------------------------------------------ | --------- |
| correct deployer, deployed **before** 2026-05-12 | **1,565** |
| correct deployer, deployed **after**             | **0**     |

So the correct rows are the ones 0255's Phase-2 migration rewrote by hand
(2,825 rows corrected from CH-internal data), and **not a single contract
deployed since the parser fix shipped has the override resolved correctly**.

Two readings, and the fix must start by telling them apart:

1. the parser fix does handle plain envelopes but never sees the fee-bump
   nesting, and every recent deployment on this launchpad happens to be
   fee-bumped; or
2. the fix is not live at all on this path.

Either way the defect is **producing new wrong rows continuously**, not sitting
in history. Factory-style deployment is common, not exotic: **22.8% of
invocations run through a contract caller** (2.75M of 12.1M in a 50k-ledger
window).

## Implementation

- [ ] Confirm the root cause above on a NON-fee-bump envelope where a contract
      is created inside a `ContractFn` call — the bug should reproduce there
      too, which would prove fee-bump is incidental.
- [ ] Pass the effective operation source (`op.source_account ?? tx_source`)
      into `extract_contract_deployments`, rather than the bare `tx_source`.
      That covers contracts born inside a contract call, which no auth-tree walk
      can see.
- [ ] Count affected rows in prod; decide backfill vs fix-forward (0255's own
      Phase 2 corrected 2,825 rows from CH-internal data — check whether the
      same is possible here or whether it needs an XDR re-parse).
- [ ] Regression test with a real fee-bump envelope fixture — the bug is
      invisible to any test built from a plain (non-fee-bump) transaction.

## The 0255 backfill was a one-off SQL swap, and the boundary is a landmine

The obvious question: if a live-parser bug writes wrong deployers, why is
**everything before 2026-05-12 correct and everything after wrong**? A live bug
should corrupt going forward, not leave a clean historical band.

Answer, in two parts.

**1. The "correct" old rows are not the parser's work — they are a hand-run SQL
migration.** 0255 Phase 2 (operator session, 2026-05-22, `stkrolikiewicz`) did
**not** re-parse XDR. It built a staging table `soroban_contracts_staging_0255`
by JOINing the correct op-source out of **CH-internal data**
(`operations_appearances`, which already had the per-op `source_account` for the
3,020 contracts that carried an explicit override), verified row-count parity
(live = staging = 321,364), and did an atomic **`EXCHANGE TABLES`** swap. 2,825
rows corrected. No `backfill-runner` subcommand, no reusable script, no runbook
(`docs/runbooks/0255_deployer_id_backfill_migration.md` was never written — the
0255 archive says so explicitly). It is a manual `EXCHANGE TABLES` migration, run
once. Its own completion note states it "corrects the EXISTING backfill snapshot
only — it does not preempt future writes."

**2. It survives only because no full reindex has re-parsed that range since — and
that is fragile.** `soroban_contracts` is `ReplacingMergeTree(wasm_uploaded_at_ledger)`
and is **unmerged** (duplicate rows persist until a merge that may never come). If
a full `run --reindex` re-parsed the 50M–62M deployment range with the current
(still-buggy) parser, it would **insert a second row** per contract carrying the
wrong deployer at the same `wasm_uploaded_at_ledger` version — an RMT version tie,
whose winner is non-deterministic. Measured 2026-07-23 to prove this has not
happened:

- the 1,565 correct (`GCNP4JVZ…`) contracts each have **exactly one raw row**;
- **zero** of them carry a second row with a different deployer.

A reindex would have left a second row. There is none. So **no full S3
reindex/reingest has touched the 50M–62M contract-deployment range since the 0255
swap** — that, and only that, is why the boundary is still clean.

**Consequence that raises this bug's priority:** the hand-fixed history is a
landmine. The day anyone runs a full historical reindex (0429-class work, a
schema migration, a disaster-recovery re-ingest) over that range with the parser
unfixed, the 1,565 correct rows flip to wrong and the boundary is erased in the
wrong direction. **Fixing the parser is therefore a prerequisite for ever running
a safe full reindex**, not just a forward-correctness fix. Do the parser fix
first; only then is a one-shot reindex of the affected range the clean way to
correct both the post-boundary wrong rows and (harmlessly) re-derive the
pre-boundary ones.

## Acceptance Criteria

- [ ] A fee-bump deployment with an op-source override stores the op source.
- [ ] Verified on ≥10 contracts against raw RPC XDR (NOT Horizon — see history).
- [ ] Existing wrong rows counted, and either corrected or explicitly deferred
      with a reason.
- [ ] The correction of history is done by re-parse **after** the parser fix
      lands — never a reindex before it (that would clobber the 1,565 correct
      pre-boundary rows). Record the ordering in `docs/backfills.md` if a reindex
      is scheduled.
- [ ] Docs updated — `N/A` unless the deployer semantic lands in
      `docs/architecture/**`; the definition itself is recorded in 0256.
- [ ] API types regenerated — `N/A` (no API surface change; the column already
      exists).

## Verification method

```bash
# 1. get the envelope (RPC retention ~7 days; oldest ledger was 63,476,197)
curl -s -X POST https://mainnet.sorobanrpc.com -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getTransaction","params":{"hash":"<TX_HASH>"}}' \
  | jq -r .result.envelopeXdr > env.b64

# 2. decode with the official CLI
stellar xdr decode --type TransactionEnvelope --input single-base64 \
  --output json-formatted < env.b64

# 3. compare tx_fee_bump.tx.inner_tx.tx.tx.operations[0].source_account
#    against soroban_contracts.deployer_id
```
