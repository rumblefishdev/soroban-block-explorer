---
id: '0418'
title: 'REFACTOR/ARCH: consolidate asset-identity vocabulary in `domain` + module-conventions ADR + split god-modules (state.rs / stage.rs)'
type: REFACTOR
status: backlog
related_adr: ['0031', '0032']
related_tasks: ['0393', '0414', '0398']
tags:
  [
    'refactor',
    'architecture',
    'domain',
    'xdr-parser',
    'phase-future',
    'effort-large',
    'priority-medium',
  ]
links: []
history:
  - date: 2026-07-20
    status: backlog
    who: karolkow
    note: 'Spawned after the 0393 AssetIdentity saga (3 iterations to land the EventAsset/LedgerAsset split). Root cause is architectural, not one enum: asset identity is re-invented across the parser, the shared `domain` kernel is underused, and two god-modules (state.rs 3290 / stage.rs 2568 LOC) prevent files from being self-describing. NOT a hexagonal/DDD rewrite (overkill for a data pipeline) — targeted consolidation + conventions + splits.'
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Inherited §D from 0398, which closed today recommending this task as the
      home for the rename.** 0398 was investigation-only and delivered its
      verdict: the `contract_id` naming collision is real — `String` StrKey in
      `soroban_contracts` / `soroban_contract_metadata`, `Int64` surrogate in 11
      other tables, so an FK named `contract_id` joins `soroban_contracts.id` and
      never `soroban_contracts.contract_id`. It already cost debugging time in
      0364.
      Recorded here because the hand-off was one-way until now: 0398 said "fold
      it into 0418" while this task had no idea. Without the cross-link the
      recommendation would have died in an archived file.
      Two things 0398 measured that matter for planning: the rename `ALTER` is
      metadata-only (148,440 + 3,850 rows) but touches **85 call sites in
      `stage.rs` and 21 in `crates/api`** — the same files this task and 0414
      already open, which is exactly why it was deferred rather than done
      standalone. And the three-name surrogate storage is **deliberate**, not
      redundancy to consolidate away: `ids::{account,contract,address}_id` are
      byte-identical and the distinct names carry intent at the call site.
---

# REFACTOR/ARCH: asset-vocabulary consolidation + module conventions + god-module split

## Summary

The 0393 `AssetIdentity` saga (three iterations to land a clean shape) was a
**symptom**, not the disease. The macro-architecture is sound — crate boundaries
(`domain` core + `xdr-parser` / `db-clickhouse` / `api` / `indexer` /
`enrichment-*` adapters) are a compile-enforced ports-&-adapters-lite. The pain is
three concrete, targeted things below. **Explicitly NOT in scope: a hexagonal / DDD
paradigm rewrite** — the domain is a thin data pipeline (XDR → rows → JSON), so the
formal machinery would be over-engineering; the crate layering already delivers most
of its benefit for free.

## A. Asset-identity vocabulary is re-invented; the shared kernel is underused

`crates/domain` IS the shared kernel (`domain/src/asset.rs`, `domain/src/enums/{asset_type,token_asset_type}.rs`),
yet "an asset is native / credit / contract" is re-modelled at least **3× in the
parser**, each with its own producer and its own inline `→ surrogate` match:

- `AssetRef { Native, Credit }` — `xdr-parser/src/asset_appearances.rs` (op-declared).
- `SacAssetIdentity { Native, Credit }` — `xdr-parser/src/types.rs` (SAC-wrapped classic).
- `EventAsset` / `LedgerAsset` — event vs ledger (0393; now cleanly per-domain).

Verified (2026-07-20): all extract `code`/`issuer` identically (`asset_code_str` +
G-StrKey) → **consistent surrogates, no latent bug**. So the duplication is benign,
not broken — which is why this is `priority-medium`, not a fix.

The same `Native → NATIVE_ASSET_ID / Credit → credit_asset_id` resolution is inlined
**~5× in `stage.rs`** (lines ~1046, 1668, 1692, 2182-ish event, 2241-ish ledger) +
`SacAssetIdentity → TokenAssetType` in `state.rs`.

Work:

- One shared resolver `fn(classic-asset-parts) -> asset_id` (or a canonical
  `domain::Asset`) that the inline sites call — collapse the ~5 copies to one.
- Decide **selectively** which of the parser enums genuinely share the vocabulary vs
  are legitimately-distinct domain types (per the 0393 devils-advocate: structural
  similarity ≠ merge; `AssetRef`/`SacAssetIdentity` are semantically distinct and
  were deliberately left separate). Consolidate only the real overlap; do NOT blindly
  merge distinct domain concepts (false-abstraction / wrong-DRY trap).

## B. No written "where does X live" convention

Organic growth means new concepts land wherever, so duplicates sprout (see A). Write
a short ADR (module-map + rules): **cross-boundary types live in `domain`**; each
adapter crate owns its own I/O vocabulary; per-domain asset enums (`AssetRef`,
`EventAsset`, `LedgerAsset`) are the sanctioned pattern. Cheap insurance against the
next AssetIdentity saga. Fits the ADR-0032 evergreen-docs discipline.

## C. God-modules block self-describing files

Biggest source files (2026-07-20): **`xdr-parser/src/state.rs` = 3290 LOC**,
`db-clickhouse/src/persist/stage.rs` = 2568, `db-clickhouse/src/persist/tests_cross.rs`
= 2413, `xdr-parser/src/invocation.rs` = 1580. A 3000-line file cannot "say what's
inside." `stage.rs` split is already **task 0414**; this task adds **`state.rs`**
(the larger offender — extract_assets / accounts / LPs / NFTs / balances / SAC
identities all in one module) as its twin.

## D. `contract_id` names two different things (handed over from 0398)

0398 investigated the contract-surrogate data model and closed with
**"document now, rename here"** — this task is the "here". Its finding:

- `contract_id` is a **`String`** (the real `C…` StrKey) in `soroban_contracts`
  and `soroban_contract_metadata`, but an **`Int64`** (the cityhash64 surrogate
  _of_ that StrKey) in 11 other tables.
- So an FK named `contract_id` joins `soroban_contracts.`**`id`**, never
  `soroban_contracts.contract_id`. This already cost debugging time in 0364.
- The three-name storage (`contract_id` / `sac_contract_id` /
  `caller_contract_id`) is a **deliberate shared surrogate space**, not
  redundancy — `ids::{account,contract,address}_id` have byte-identical bodies.
  Do not "consolidate" them away; they carry intent at the call site.

Costed by 0398: the `ALTER` is metadata-only (148,440 + 3,850 rows), but the
call sites are **85 in `stage.rs` + 21 in `crates/api`**. That is why it was
deferred rather than done standalone — a wide mechanical diff through the same
files this task and 0414 already touch. Renaming the `String` column to
`contract_strkey` is the obvious shape; the surrogate columns keep their names.

A warning comment is already in `init.sql` above `CREATE TABLE
soroban_contracts` (landed by 0398) — remove it if this rename happens, since
it documents a trap that would no longer exist.

## Acceptance Criteria

- [ ] The ~5 inline `Native/Credit → surrogate` sites resolve through one shared
      function; no behavioural change (surrogates identical — pin with a test).
- [ ] A documented decision on which parser asset enums consolidate vs stay distinct
      (with the semantic-distinctness rationale), and any consolidation done.
- [ ] Module-conventions ADR written (where cross-boundary types live; per-domain
      asset-enum pattern sanctioned).
- [ ] `state.rs` god-module split into cohesive modules (twin of 0414's `stage.rs`).
- [ ] Decision recorded on the `contract_id` naming collision inherited from
      0398 (§D): rename `soroban_contracts.contract_id` → `contract_strkey`, or
      keep documenting it. If renamed, drop the `init.sql` warning comment.
- [ ] No hexagonal/DDD ceremony introduced — targeted changes only.
