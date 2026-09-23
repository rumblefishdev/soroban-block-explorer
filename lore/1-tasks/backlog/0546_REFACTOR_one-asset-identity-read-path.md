---
id: '0546'
title: 'REFACTOR: one asset-identity read path (surrogate → display identity)'
type: REFACTOR
status: backlog
related_adr: ['0051']
related_tasks: ['0374', '0344', '0345', '0496', '0231']
tags: [backend, clickhouse, api, assets, read-path, priority-low, effort-medium]
links: []
history:
  - date: '2026-08-28'
    status: backlog
    who: karolkow
    note: >
      Filed from 0374 step 13: resolving a display identity from a surrogate
      touches assets + asset_sac (+ enrichment for names/icons), and every
      consumer used to hand-assemble the joins. TRIGGER-GATED — see below.
  - date: '2026-09-23'
    status: backlog
    who: karolkow
    note: >
      Trigger met: three consumers share `common::asset_identity` (account
      balance changes, pool legs, global search). Scope extended from the
      backend read surface to the wire and the frontend — the naming of an
      asset is decided in seven places and native XLM is spelled four ways.
      Inventory below. Scheduled after the 0374 read-half split, as its own
      PR, not folded into a pool PR.
---

# REFACTOR: one asset-identity read path

## The smell, named

Getting "what do I call this asset" from an `Int64` surrogate takes three
tables. The write-side split is measured necessity, not mess — `asset_sac`
and `asset_enrichment` exist because the versionless-RMT `assets` rewrite
clobbered mutable columns (ADR 0051 storage correction; task 0231) — but the
read side made every consumer re-derive the same joins.

## What exists already — do not rebuild it

The house pattern is the **0344/0345 id-IN resolver**: point-seek a dimension
by an id list instead of hash-joining the whole table. Two instances live:

- `resolve_accounts` (accounts dimension, 0345)
- `resolve_asset_identities` in `common::asset_identity` (0374 split, PR 2a —
  shared by account balance changes, pool legs and search)

## The closure this task builds (when triggered)

One canonical `asset_identities` read surface — a view or dictionary keyed by
surrogate: `id → (family, code, issuer_id, contract strkey, decimals, name,
icon)` — assembled ONCE from assets + asset_sac + asset_enrichment +
soroban_contract_metadata, with the AMT/RMT read protocols (GROUP BY + max /
argMax by version) applied in exactly one place. Existing resolvers re-point
at it; new consumers `dictGet`/join it.

## Trigger — depth-first gate (Karol, 2026-08-28)

**Do not start until a THIRD consumer needs richer identity fields** than a
current resolver serves. Two instances are a pattern; a third is the signal
to consolidate. Until then this file is the address where the smell is
recorded, so nobody re-litigates it from scratch.

## Extension (2026-09-23): one display name, API to frontend

Measured while reviewing the 0374 pool-legs PR. The backend read surface above
is half of it; the other half is that the API sends the PARTS of a name and
every screen assembles its own.

**Who decides what an asset is called, today:**

| Layer                | Where                                                                        | Rule                                                                                                 |
| -------------------- | ---------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| SQL                  | `common::asset_identity::shown_code_sql` (search, assets list, pools filter) | native → `XLM`, else code — for MATCHING                                                             |
| Rust                 | `common::asset_identity::leg_label` (search labels)                          | code → symbol → truncated contract                                                                   |
| Rust                 | account balance changes                                                      | folds code-or-symbol into one `asset_code` field                                                     |
| Frontend             | `assetDisplayCode` (asset page, pool legs, balance-change cell)              | native → code → symbol → truncated contract                                                          |
| Frontend             | `AccountBalances.shape()`                                                    | its own ladder: a symbol-less token reads `—`, where every other screen shows its truncated contract |
| Frontend             | `humanizeOp`, `sacAsset`                                                     | `asset_code ?? 'XLM'` / `?? '?'`                                                                     |
| Frontend (`libs/ui`) | `formatTokenAmount(amount, code)`                                            | a missing code silently renders `XLM` — a trap for any caller passing `null`                         |

**Native XLM, four spellings:** empty `asset_code` in the database;
`asset_code: null` + `asset_type_name: 'native'` in the assets, balances and
pool-leg responses; the string `'native'` in balance changes and operation
details; `asset_code: null, issuer: null` for a contract's SAC asset.

**Storing `XLM` as native's code was checked and rejected.** It would fix only
the SQL row: `asset_code` is in the sort key of `assets`, `asset_sac` and
`asset_enrichment` (no in-place `ALTER … UPDATE`), the indexer would rewrite it
back, and Stellar itself has no code for native (XDR, SDKs, our API all say
`null`). The native surrogate does not depend on the code (`ids::asset_id`
hashes `"native"`), but that does not remove the other costs.

**The fix this extension targets:** every response that carries an asset also
carries ONE ready display name, computed by ONE Rust function over the resolved
identity; the frontend renders it and its ladders go away. `formatTokenAmount`
stops defaulting to `XLM`.

## Acceptance Criteria

- [ ] one read surface serves every surrogate→identity consumer
- [ ] AMT/RMT read protocols live in exactly one definition
- [ ] `resolve_accounts` / `resolve_asset_identities` re-pointed or retired
- [ ] read cost measured before/after on the union pool list and account page
- [ ] every asset-carrying response has one display-name field from one Rust function
- [ ] frontend renders that field; `assetDisplayCode`, `AccountBalances.shape()`'s ladder and the `?? 'XLM'` / `?? '?'` fallbacks are gone
- [ ] `formatTokenAmount` no longer turns a missing code into `XLM`
- [ ] native XLM spelled one way on the wire
- [ ] **Docs updated** — `docs/architecture/backend/**` and `frontend/**` data contracts; API types regenerated
