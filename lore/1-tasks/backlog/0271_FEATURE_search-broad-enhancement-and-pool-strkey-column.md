---
id: '0271'
title: 'Search broad enhancement: asset.name + nft.collection_name + pool L-strkey prefix matching'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0270', '0264', '0257']
tags:
  [
    'backend',
    'database',
    'search',
    'priority-low',
    'effort-medium',
    'phase-future',
    'deferred-from-0270',
  ]
links:
  - 'Parent: lore/1-tasks/active/0270_FEATURE_search-strkey-canonical-output-and-redirect-coverage.md'
  - 'Industry research (cross-explorer survey): summarised in 0270 session transcript'
history:
  - date: '2026-05-27'
    status: backlog
    who: karolkow
    note: >
      Spawned during 0270 session redesign discussion. User decided to keep
      0270 minimalist (drop muxed/asset-composite/ledger-numeric backend
      branches, FE-side ledger redirect, no broad-search field expansions,
      pool broad stays at full-L-strkey-only). This task collects the three
      deferred enhancements so they can be picked up after the 0257
      pre-launch audit (Wave 6) closes. Detailed rationale + research
      findings preserved inline below so future sessions don't need the
      0270 chat transcript.
---

# Search broad enhancement + pool strkey column

## Summary

Three deferred broad-search enhancements emerged during the 0270
redesign discussion but were intentionally cut from 0270's scope to
keep that task minimalist and unblock 0257 Wave 6:

1. **Asset broad search by `name`** — add a GIN trgm index on
   `assets.name`, extend the `asset_hits` CTE in
   `crates/api/src/search/queries.rs` with `OR a.name ILIKE '%' || $1 || '%'`.
2. **NFT broad search by `collection_name`** — replace the existing
   btree `idx_nfts_collection` with a GIN trgm index, extend the
   `nft_hits` CTE with `OR n.collection_name ILIKE '%' || $1 || '%'`.
3. **Pool broad search by partial L-strkey prefix** — denormalise
   the L-strkey form of `liquidity_pools.pool_id` into a new
   indexed text column so `LIKE 'LAB%'` works the same way it works
   for accounts (`G…`) and contracts (`C…`).

All three are pure additions to broad search; none change the
direct-redirect path or the wire-shape contract.

## Context

### Why each was deferred from 0270

**0270 sat at the intersection of two scope explosions.** The original
deferred batch from 0264 (search strkey canonical output + redirect
coverage gaps) was already cross-cutting — classifier, redirect, SQL,
DTO, FE routing — and during the redesign the user pushed for **maximum
minimalism**, cutting muxed M decode, asset composite redirect, and
ledger numeric backend redirect. The decision was: keep 0270 to the
**actually needed** scope (canonical strkey on the wire, NFT composite
routing, FE ledger redirect, L-strkey decode in classifier) and spawn
this follow-up for the **nice-to-have** broad-search field expansions.

The 0257 frontend comprehensive audit is the immediate unblocker — its
Wave 6 (Playwright marathon) needs 0270 landed to clear F-L-1 + F-K-4.
This task is intentionally low-priority so it doesn't compete.

### Industry research findings used in the decision

Cross-explorer survey conducted during 0270 (stellar.expert, etherscan,
solscan, opensea, magiceden, mempool.space, blockchair):

- **Bare asset/token name (`USDC`, `BAYC`) always → list, never redirect.**
  Code/name collisions are universal. Confirms `name` field is a
  legitimate broad-search target (this task) but never a redirect
  target (out of scope here).
- **NFT collection name → list, NFT contract address → redirect.**
  `collection_name` as a broad-search field matches user mental model
  ("show me BoredApes"). Partial substring matching expected — GIN trgm
  is the right tool.
- **Partial transaction hash search has zero industry precedent.**
  Reinforces 0270's decision to drop tx from broad bucket entirely.
- **Pool L-strkey prefix matching** — no explorer surveyed exposes
  partial pool ID matching today, but every explorer with prefix-typed
  identifiers (G/C strkeys on Stellar; 0x addresses on EVM; base58 on
  Solana) supports partial prefix matching on the **flat text** form of
  the identifier. The Stellar L-strkey is structurally identical to
  G/C; only our DB storage shape (raw 32-byte BYTEA, no text mirror)
  prevents the same treatment.

### Why partial L-prefix can't be done FE-only

base32 + CRC16 encoding does **not** decode partially:

- 5 bits per base32 char doesn't align with bytes (8 chars = ~5 bytes;
  partial chars give fractional bytes — undefined boundary).
- The `L` prefix character is the version byte `0x60` encoded; matching
  "starts with `L`" in base32 ≠ matching "starts with `0x60`" in bytes
  (every pool L-strkey has the same first byte 0x60, since version
  byte is a constant for pool IDs).
- CRC16 is at the end of the 35-byte payload; partial input fails the
  `stellar_strkey::LiquidityPool::from_string()` checksum check.

Therefore partial L-strkey → partial hex bytes is structurally
impossible. The fix has to live in DB storage.

### Why same problem doesn't affect accounts / contracts

`accounts.account_id` and `soroban_contracts.contract_id` are
`VARCHAR(56)` columns storing the full strkey as **text** (per ADR
0008 strkey adoption). Postgres `text_pattern_ops` btree indexes serve
`LIKE 'GAB%'` and `LIKE 'CCR%'` natively. The same approach works for
pools but requires adding the text column.

## Implementation

### Phase 1 — Asset broad by name

**Migration:**

```sql
CREATE INDEX idx_assets_name_trgm
    ON assets USING GIN (name gin_trgm_ops)
    WHERE name IS NOT NULL;
```

`pg_trgm` extension is already enabled (used by `idx_assets_code_trgm`
and `idx_nfts_name_trgm`).

**Code change** in `crates/api/src/search/queries.rs::fetch_search`
inside `asset_hits` CTE:

```sql
AND (
    (a.asset_code IS NOT NULL AND a.asset_code ILIKE '%' || $1 || '%')
 OR (a.asset_type = 0 AND ($1 ILIKE 'xlm' OR $1 ILIKE 'native'))
 OR (a.name IS NOT NULL AND a.name ILIKE '%' || $1 || '%')   -- NEW
)
```

**Test:** insert asset with `asset_code = 'CTKN'`, `name = 'CoolToken'`;
search `q = 'cool'` finds it via name path; search `q = 'CTKN'` finds
it via code path; both routes deduped at row level (same `assets.id`).

### Phase 2 — NFT broad by collection_name

**Migration:**

```sql
DROP INDEX idx_nfts_collection;
CREATE INDEX idx_nfts_collection_trgm
    ON nfts USING GIN (collection_name gin_trgm_ops)
    WHERE collection_name IS NOT NULL;
```

The current btree only serves exact match. GIN trgm covers partial +
exact. Btree dropped because no current query path needs exact-only.

**Verify no other consumers of the btree** before dropping. Quick grep:

```bash
rg "ORDER BY collection_name|WHERE collection_name ?=" crates/
```

Add a partial-index variant if a hot exact-equality query exists
elsewhere (unlikely; NFT enrichment code keys on `(contract_id, token_id)`).

**Code change** in `nft_hits` CTE:

```sql
AND n.name IS NOT NULL
AND (
    n.name ILIKE '%' || $1 || '%'
 OR (n.collection_name IS NOT NULL AND n.collection_name ILIKE '%' || $1 || '%')
)
```

**Test:** insert 2 NFTs same collection `'BoredApes'`, different names;
search `q = 'bored'` returns both rows; search `q = 'ape #7'` returns
only the matching token by name path.

### Phase 3 — Pool L-strkey denormalised column

**Decision point at picking-up time:** choose between Path A
(generated column) and Path B (regular column + backfill + trigger).

#### Path A — Generated column (preferred if SQL function lands cleanly)

```sql
-- Step 1: write a PL/pgSQL function that mirrors
-- crates/api/src/common/strkey.rs::pool_id_hex_to_strkey
CREATE OR REPLACE FUNCTION pool_id_to_strkey(p BYTEA) RETURNS TEXT
    LANGUAGE plpgsql IMMUTABLE PARALLEL SAFE AS $$
DECLARE
    -- 35-byte payload: [version=0x60][p (32 bytes)][CRC16 (2 bytes)]
    payload BYTEA;
    crc INTEGER;
BEGIN
    -- ...base32 + CRC16 implementation (see SEP-23)...
END;
$$;

-- Step 2: add generated column
ALTER TABLE liquidity_pools
    ADD COLUMN pool_id_strkey TEXT
    GENERATED ALWAYS AS (pool_id_to_strkey(pool_id)) STORED;

-- Step 3: btree index for prefix LIKE
CREATE INDEX idx_pools_strkey
    ON liquidity_pools (pool_id_strkey text_pattern_ops);
```

**Risk:** PL/pgSQL implementation of base32 + CRC16 = ~100 LOC SQL,
needs careful testing against Rust `stellar_strkey::LiquidityPool::to_string()`
output. Recommend cross-checking every byte combination on a representative
sample (all-zero, all-FF, mainnet pool IDs).

#### Path B — Regular column + backfill + Rust-side write logic

```sql
-- Step 1: add column
ALTER TABLE liquidity_pools ADD COLUMN pool_id_strkey TEXT;

-- Step 2: backfill via one-shot script (Rust binary in
-- crates/db-tools/ or similar) that reads pool_id, computes strkey
-- via the existing pool_id_hex_to_strkey helper, writes back.
-- Lock concern: liquidity_pools is small (~thousands of rows),
-- a single SELECT + UPDATE batch is fine without explicit locks.

-- Step 3: NOT NULL constraint after backfill
ALTER TABLE liquidity_pools ALTER COLUMN pool_id_strkey SET NOT NULL;

-- Step 4: btree index
CREATE INDEX idx_pools_strkey
    ON liquidity_pools (pool_id_strkey text_pattern_ops);

-- Step 5: BEFORE INSERT trigger OR modify every INSERT site to compute
-- the strkey app-side. Trigger approach uses the same PL/pgSQL function
-- as Path A (but stored as a trigger, not generated column).
-- App-side approach adds 1-2 LOC at each INSERT site (indexer write
-- path) and avoids new SQL function entirely.
```

**Risk:** synchronisation. If the trigger lives in SQL and the Rust
encoder ever diverges (e.g. CRC16 table tweak), rows can drift. The
app-side INSERT approach is safer because there is exactly one
implementation of `pool_id_hex_to_strkey` (in Rust).

**Recommendation at pickup time:** Path B with app-side write logic.
Simpler (no SQL base32+CRC16), test surface stays in Rust, generated-
column savings don't matter at pool-table scale.

#### Code change in `fetch_search`

Replace exact-match `pool_hits` CTE with prefix match. Drop dependency
on `hash_bytes` for pool broad bucket — instead use the new
`strkey_prefix` channel for L-prefix:

```sql
pool_hits AS (
    SELECT
        'pool'::text                       AS entity_type,
        lp.pool_id_strkey                  AS identifier,
        (
            COALESCE(lp.asset_a_code, 'XLM') || ' / ' ||
            COALESCE(lp.asset_b_code, 'XLM')
        )::text                            AS label,
        NULL::bigint                       AS surrogate_id,
        NULL::bool                         AS successful,
        NULL::timestamptz                  AS last_activity_at,
        NULL::varchar                      AS contract_id,
        NULL::varchar                      AS token_id
    FROM liquidity_pools lp
    WHERE $10 = TRUE
      AND (
          ($2 IS NOT NULL AND lp.pool_id = $2)                       -- full L-strkey via hash_bytes (existing path)
       OR ($N IS NOT NULL AND lp.pool_id_strkey LIKE $N || '%')      -- partial L-prefix (NEW)
      )
    LIMIT $4
)
```

Classifier gains a new channel `strkey_l_prefix: Option<String>`
populated when q starts with `L` and is 2–55 chars, all-base32. Full
56-char L-strkey continues to populate `hash_bytes` via existing decode
branch (redirect path stays unchanged).

`fetch_search` binds the new param. Classifier unit tests cover
partial-L prefix branch.

**Test:** insert 3 pools; search `q = 'LAB'` returns pools whose
strkey starts with `LAB`; search full L-strkey returns same redirect
behaviour as today.

### Phase 4 — Update docs

`docs/architecture/database-schema/tables/liquidity_pools.md` —
document the new column + index, link this task.

`docs/architecture/api/url-conventions.md` — "Search input" section,
mention partial L-prefix support now works (matches G/C parity).

## Acceptance Criteria

### Backend

- [ ] Migration `idx_assets_name_trgm` lands; verify with `\d+ assets`
- [ ] Migration `idx_nfts_collection_trgm` lands; old btree dropped
- [ ] Pool strkey column path chosen (A or B) + migration lands
- [ ] `asset_hits` CTE adds `name ILIKE` branch
- [ ] `nft_hits` CTE adds `collection_name ILIKE` branch
- [ ] `pool_hits` CTE adds partial L-prefix branch
- [ ] Classifier extended with `strkey_l_prefix` channel
- [ ] All new branches covered by unit + integration tests
- [ ] Backfill script for pool strkey column (Path B) — runbook + dry-run output

### Cross-cutting

- [ ] OpenAPI regen committed (CTE field shape unchanged, but worth running)
- [ ] `cargo test -p api` + `nx run web:typecheck` green
- [ ] `docs/architecture/database-schema/tables/liquidity_pools.md` updated
- [ ] `docs/architecture/api/url-conventions.md` updated (per ADR 0032)
- [ ] Optional ADR if Path A is chosen (PL/pgSQL base32+CRC16 implementation
      is architecturally novel for the project — worth recording)

## Future Work

None planned beyond this task. After these three enhancements ship,
broad search has parity across all six entity types: text-typed
identifiers all support partial prefix, name-typed fields all support
ILIKE substring matching backed by trgm. The next enhancement layer
would be ranking / boost-by-relevance, which is out of scope here.

## Notes

- **Pool table size** at picking-up time is the deciding factor between
  Path A and Path B. If `liquidity_pools` has grown to millions of rows
  (e.g. derivative pools, expanded AMMs), backfill cost shifts the
  calculus toward Path A. Today (2026-05-27) the table is ~thousands of
  rows so Path B is comfortable.
- **Pickup order:** Phases 1 and 2 are independent ~30-min changes and
  can ship as a single PR. Phase 3 is the bulk (~1 day) and probably
  warrants its own PR. Split if reviewer load is a concern.
- **Audit gate:** verify no breaking change to `SearchHit` wire shape
  (none of the three phases adds new columns — pool `identifier` was
  already strkey since 0264, this task just lets a partial-L query
  return non-empty rows).
- **F-L-1 / F-K-4 from 0257:** independent of this task. Both are closed
  by 0270.
