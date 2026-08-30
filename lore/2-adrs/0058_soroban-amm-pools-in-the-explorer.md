---
id: '0058'
title: 'Soroban AMM pools in the explorer — registry, state facts, share relation, one list'
status: accepted
deciders: [karolkow]
related_tasks: ['0374', '0496']
related_adrs: ['0051', '0056', '0057']
tags: [soroban, amm, liquidity-pools, clickhouse, api]
links: []
history:
  - date: '2026-08-29'
    status: accepted
    who: karolkow
    note: >
      Records the durable decisions from task 0374 (Aquarius / router-family
      AMM support, issue #405). The per-step measurements and the full
      verification record stay in the task; this ADR carries what future work
      must not re-litigate.
---

# ADR 0058: Soroban AMM pools in the explorer

## Context

Classic (CAP-38) liquidity pools are host-maintained ledger entries the
explorer has always indexed. Soroban AMM pools are ordinary contracts: the
ledger holds no pool table, no reserve column, no share registry — everything
is contract-authored state and events, per protocol. Issue #405 asked for
Aquarius and Soroswap. Task 0374 built the router family (Aquarius's shape)
end to end, depth-first: only what this family needs, measured on the full
mainnet population at every step.

## Decisions

### 1. Discovery is shape-driven, per deployment — never address-driven

Pools enter the registry by decoding every `add_pool` event that satisfies the
registration shape, from ANY deployment (`detect_pool_registrations` in
`xdr-parser`). A hardcoded router address list loses ~6% of live pools
(measured); a shape decode loses none and picks up new routers with no code
change. The registering router is stored as `liquidity_pools.deployment_id`
(contract surrogate).

**Protocol labels are attribution, resolved at read time** from
`deployment_id` against a verified-operator list in the API. Two live routers
share Aquarius's WASM byte-for-byte with all seven admin roles disjoint
(measured), so code identity does not establish operator identity: the
vendor-documented router labels `aquarius`, the other stays indexed and
UNLABELLED. A label fix is a code change, never an UPDATE.

### 2. One pool dimension, two id worlds

Soroban pools are rows in the same `liquidity_pools` table
(`pool_kind = 1`), because the list endpoint must union both worlds and a
dimension shares its grain (one row = one pool). New columns: `pool_kind`,
`legs Array(Int64)` (token-CONTRACT surrogates in emission order — 3- and
4-leg stable pools exist, a pair cannot hold them), `deployment_id`,
`pool_type_raw` (verbatim event sym — three vendor vocabularies exist for one
shape; folding them is read-time interpretation), `share_token_id`,
`subpool_salt`, `init_args` (raw — three arg vocabularies measured).

The 32 id bytes of a soroban pool are a CONTRACT address payload. The API
renders them as `C...` and accepts both `L...` and `C...` on pool routes; an
`L...` render of contract bytes would be a well-formed WRONG key, so it is
never minted.

### 3. Reserves are ledger STATE at the chain's grain — `pool_state_changes`

Event arithmetic failed its oracle on 6/49 pools (measured), so reserves come
from ledger-entry changes, written to `pool_state_changes
(pool_id, ledger_sequence, application_order, transaction_id, change_index,
reserves Array(Int128), plane_id)` — sort key
`(pool, ledger, application_order, change_index)`. Two on-chain layouts feed one table:

- **fungible** pools (constant/stable) write `PoolData[pool]` on the
  deployment's shared _plane_ contract — the vector is stored VERBATIM
  (concentrated-era tails exist; readers slice by leg count, never by vector
  length);
- **concentrated** pools write `Reserve0`/`Reserve1` on their own instance —
  the plane holds their `PoolData` only at registration (discovered by a
  bidirectional anti-test against `update_reserves` events; the T3 pilot
  predated the first concentrated pool).

The full key matters: `(pool, ledger)` alone collapses 23.5% of rows (up to
12 writes/ledger measured) — and the intra-ledger component must be
`application_order` (the transaction's position in its ledger), never the
`transaction_id` surrogate: a hash sorts randomly, and "latest reserves"
picked by it returned an intermediate write on 127 of 1,410 real pairs
(caught by the full-pipeline e2e before the production DDL existed). This table is deliberately named WITHOUT a family
prefix: it is the target state-fact shape (chain grain, verbatim integers,
single concern), and classic snapshot history would join INTO it if the two
snapshot models ever unify — never the reverse. The classic
`liquidity_pool_snapshots` stays as-is until a consumer needs one
cross-family history read (trigger recorded in task 0374).

### 4. The share-token relation is a SIDE table — `pool_share_tokens`

The pool→share-token relation is derived from the pool instance's
`TokenShare` key (chain state), cross-checked by the SEP-41 mint rule from
deposit transactions. It lives in `pool_share_tokens (pool_id,
share_token_id, derived_at_ledger)` — the `asset_sac` pattern — because the
deriving path knows only `(pool, token)`, and a partial row written into the
RMT registry would replace the full registration on merge. Versioned by
sighting ledger so a share-token migration (13 pools re-pointed theirs,
measured) converges on the newest, matching on-chain `share_id()`.
Concentrated pools mint nothing and never appear; their positions are NFTs
(future work).

A Soroban token IS an LP share exactly when it appears in this relation —
never an `assets` column (ADR/task 0496: `AssetFamily` stays
native/classic/soroban; "LP share" is a relation, not an asset type).

### 5. API: one list, explicit refusals, no misleading zeros

`GET /liquidity-pools` returns both kinds discriminated by `pool_kind`;
soroban rows publish `legs[]` (resolved through the `asset_sac` facet or the
bespoke-token `assets` row; an unresolvable leg surfaces as
`family: "unresolved"`, never a plausible empty asset) and null out the pair
fields. Values with the wrong population's truth go ABSENT, not zero:
`participant_count` is null on soroban list rows (their participants are
share-token holders in `balances`, answered per pool by the participants
endpoint); classic-only feeds (USD chart, `lp_operation_amounts` activity)
REFUSE soroban pools with an explanatory 400 instead of returning confidently
empty series. Soroban participant `shares` display scaled by the share
token's on-chain decimals, or null when the token never published them — an
unknown scale must not render raw units as if scaled.

## Consequences

- A new AMM protocol needs: nothing (registration + reserves flow in via the
  shape decode and state extraction) — plus a one-line verified-operator
  label if attribution is established. Protocols with different shapes
  (e.g. Soroswap) add their own detector/adapter, same tables.
- The union list is served by one query; per-kind branching exists only in
  enrichment and the per-pool feeds. Every pool-history endpoint carries a
  per-kind branch until the snapshot models unify — a named, accepted cost.
- `balances` is read asset-first by the soroban participants endpoint (full
  scan, measured 121.5M rows / 71 ms); a skip index on `asset_id` is the
  upgrade path if it gets hot.

## Docs updated

- `docs/architecture/database-schema/database-schema-overview.md` — updated
  (new tables + `liquidity_pools` union columns)
- `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` —
  updated (pool registration + state extraction)
- `docs/architecture/xdr-parsing/xdr-parsing-overview.md` — updated
  (`pool_router` / `pool_state` modules)
- `docs/architecture/backend/backend-overview.md` — N/A: endpoint routing and
  auth topology unchanged; wire-shape changes are documented on the DTOs and
  in the regenerated OpenAPI types
- `docs/architecture/frontend`, `infrastructure`, `security` — N/A: no
  contract change beyond the regenerated `@rumblefish/api-types`
