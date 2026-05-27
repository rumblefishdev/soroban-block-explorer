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
  - date: '2026-05-27'
    status: backlog
    who: karolkow
    note: >
      Refined Phase 3 + "Drop SearchResponse::Redirect" sections. Three
      changes: (1) recommend option C (collapse `fetch_redirect` into
      broad search, synthesise Redirect from singleton row) over option
      B (drop Redirect variant + FE shape-classifier) — C is
      non-breaking wire, preserves backend existence check, removes
      dead-code asymmetry around `tx_hits` / `pool_hits` CTEs;
      (2) drop proposed separate `strkey_l_prefix` channel — extend
      existing `strkey_prefix` to accept G/C/L uniformly, since the
      three column storage shapes are non-overlapping by first
      character; (3) document that `pool_hits` "dead code today"
      becomes load-bearing under option C (sole path for full
      L-strkey + partial-L-prefix matching).
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

Replace exact-match `pool_hits` CTE with prefix match on the new text
column. Reuse the existing `strkey_prefix` channel (already used for G
and C); after Phase 3 it accepts G/C/L uniformly because each entity's
LIKE predicate is gated by the column whose values can only start with
that letter (G-accounts vs C-contracts vs L-pools never collide):

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
    WHERE $9 = TRUE
      AND $3 IS NOT NULL
      AND lp.pool_id_strkey LIKE $3 || '%'
    LIMIT $4
)
```

Full 56-char L-strkey is just the maximum-length case of the same
prefix match — `LIKE 'LABC…56char%'` collapses to exact-match on the
unique row. No separate `hash_bytes` branch required for pools after
Phase 3.

Classifier change: extend the existing `strkey_prefix` validation to
accept inputs starting with `L` (uppercase, base32, 2–56 chars). No new
channel. The previous draft of this task proposed a separate
`strkey_l_prefix: Option<String>` channel — rejected as redundant
because account_id / contract_id / pool_id_strkey storage is
non-overlapping by first character, so a single channel feeding all
three CTE LIKE predicates naturally routes to the right one.

`hash_bytes` channel collapses to the 64-hex transaction hash case
only (the sole remaining BYTEA-storage entity).

**Test:** insert 3 pools; search `q = 'LAB'` returns pools whose
strkey starts with `LAB`; search full L-strkey returns the singleton
row (which, under the C redirect strategy below, becomes a Redirect).

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
- [ ] Classifier `strkey_prefix` validation extended to accept `L` prefix (single channel covers G/C/L; no separate `strkey_l_prefix`)
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

After these three enhancements ship, broad search has parity across
all six entity types: text-typed identifiers all support partial
prefix, name-typed fields all support ILIKE substring matching
backed by trgm. The next enhancement layer would be ranking /
boost-by-relevance, which is out of scope here.

### Consider: collapse `fetch_redirect` into broad search (RECOMMENDED — option C)

Spawned from the 0270 session deep dive (cross-explorer survey,
2026-05-27), refined in the 2026-05-27 follow-up discussion.

**Today's architecture (A — status quo):** handler calls
`fetch_redirect` first (4 sequential indexed probes: tx hash → pool
exact → account G-prefix-56 → contract C-prefix-56). On hit returns
`SearchResponse::Redirect`. On miss falls through to `fetch_search`
broad CTE. Two SQL paths to maintain, plus a `Classified::is_fully_typed()`
gate in between.

**Three industry references diverge** from this two-path shape:

- **Solana Foundation Explorer** (source-read,
  `solana-foundation/explorer/app/features/search/model/`): pure FE
  classifier with provider registry; navigates optimistically on
  shape match. No existence check.
- **Etherscan family** (etherscan.io + clones): server-side 302 but
  shape-based only, no existence check. Genesis block hash pasted
  into the bar still lands on `/tx/<hash>` with "Transaction not
  found" — destination page owns the miss UX.
- **stellarchain.io** (live-probed): pure FE classify-then-redirect.
  Bad-CRC G-strkey routes to `/address/<G>` and the detail page
  renders "Account Not Found".

**stellar.expert** does API singleton-match-before-routing — same
shape as option C below.

#### Three refactor options considered

**Option B — drop `SearchResponse::Redirect`, FE shape-classifier:**

1. Drop `fetch_redirect`, `RedirectRow`, `SearchResponse::Redirect`,
   `SearchRedirect`. Wire collapses to `{ groups: ... }`. **Breaking
   wire change.**
2. Extend `web/src/search/directRouteFor.ts` from 1 shape to 5:
   64-hex → `/transactions/<hex>`, full G-strkey → `/accounts/<G>`,
   full C-strkey → `/contracts/<C>`, full L-strkey →
   `/liquidity-pools/<L>`, bare u32 → `/ledgers/<seq>` (already).
3. FE drops `data.type === 'redirect'` useEffect and synthesis
   helpers.
4. OpenAPI regen — `SearchResponse` loses `Redirect` variant.

Cost: 404 audit on every detail page (must render `NotFoundState`
gracefully, not crash, not infinite spinner). Breaking wire shape.
Optimistic navigation — backend stops being existence authority.

**Option C — keep `Redirect` variant, synthesize from broad
singleton (RECOMMENDED):**

1. Delete `fetch_redirect`, `RedirectRow`, `Classified::is_fully_typed()`.
   `Classified` keeps `hash_bytes` + `strkey_prefix` as CTE branch
   selectors only.
2. Re-enable `tx_hits` CTE (currently dropped from `fetch_search`,
   see comment at queries.rs:227-235). Combined with the Phase 3
   `pool_hits` revival, broad search becomes 6 CTEs covering every
   entity type with text or hex identifier.
3. Handler computes `total = sum(groups.values().len())` after
   `fetch_search` returns:
   - `total == 1` → `SearchResponse::Redirect(SearchRedirect::from_hit(singleton))`
   - `total != 1` → `SearchResponse::Results { groups }`
4. Wire shape unchanged — `Redirect` variant preserved, non-breaking.
5. FE unchanged — `SearchResultsPage` redirect useEffect keeps working.

Cost: every shape-typed input now pays full 6-CTE fanout instead of
the targeted-probe short-circuit. On mainnet-scale indexes this is
single-digit ms — broad search already runs on every freetext query
today, so the SQL plan is well-trodden. Removes ~200 LOC of
redirect-only code path. New entity onboarding becomes 1-place
(add a CTE) instead of 2-place (CTE + redirect branch).

**Properties that make C the right pick:**

- **Shape-agnostic redirect rule.** "Singleton in broad → redirect"
  works for partial-prefix inputs that A cannot redirect (e.g.
  `q = "LABCDEFGHIJK…"` matching exactly one pool by prefix). A
  needs a 56-char-with-valid-CRC gate; C just counts rows.
- **Existence check preserved.** Unlike B, backend stays the
  authority on "this entity exists" — no optimistic navigation.
  No 404 audit on detail pages needed.
- **Pool_hits and tx_hits stop being dead code.** Today both CTEs
  are scaffolds (pool) or removed (tx) because `fetch_redirect`
  fires first with the same predicate, making the broad branch
  unreachable. Under C, these CTEs are the _only_ path for full
  L-strkey / 64-hex tx — they become load-bearing.
- **Mainnet-scale assumption neutralises false-redirect risk.**
  On a fully-indexed mainnet, popular asset codes (`USDC`, `BTC`)
  have many issuers — singleton on freetext input is rare and
  meaningful (NFT with unique name, sole asset under a specific
  code). Singleton = "exactly one match exists, navigate to it"
  matches user intent.
- **Hex collision non-issue.** Stellar tx hash and pool_id share
  32-byte BYTEA output space, but cryptographic collision
  probability is 2^-256. In practice a 64-hex input matches at
  most one of the two — no priority-order ambiguity.
- **Limit edge case non-issue.** `?limit=1` could theoretically
  cause "raw count == 1" by clipping, but the project does not
  expose a UI knob for `limit`; default 10 leaves enough headroom
  that singleton means true count.

**Option C-prime — classifier-gated singleton (REJECTED):**

An earlier hybrid kept the classifier as a gate
(`is_fully_typed() && count == 1 → Redirect`) to avoid
false-redirect-on-freetext-singleton. With mainnet scale + no
user-facing `?limit` knob + 2^-256 hex collision, the freetext
singleton case is either (i) impossible (popular names) or (ii)
exactly what the user wants (unique-name NFT). Gating becomes
dead weight. Pure C is simpler and equally correct.

#### Implementation sketch (option C)

```rust
// crates/api/src/search/handlers.rs

let groups = fetch_search(/* … */).await?;
let total: usize = groups.values().map(Vec::len).sum();
match total {
    1 => {
        let only = groups.into_values().flatten().next().expect("len==1");
        Json(SearchResponse::Redirect(SearchRedirect::from_hit(only)))
    }
    _ => Json(SearchResponse::Results(SearchResults { groups })),
}
```

```sql
-- crates/api/src/search/queries.rs  — fetch_search additions:
-- 1. Re-enable tx_hits CTE (removed today, see queries.rs:227-235).
--    Predicate: WHERE $? = TRUE AND $hash_bytes IS NOT NULL
--                 AND tx.hash = $hash_bytes
-- 2. After Phase 3: pool_hits switches from $hash_bytes exact-match
--    to strkey_prefix LIKE on pool_id_strkey (covered above).
-- fetch_redirect: deleted entirely.
```

Classifier change: `Classified::is_fully_typed()` deletion + extend
`strkey_prefix` validation to accept `L` (covered in Phase 3 update
above). Two methods to remove total.

#### Open decision points at pickup

- **Detail page 404 UX.** Even though C preserves existence
  check, a stale link or direct URL paste can still hit a detail
  page with a non-existent id. Worth a 30-min audit of `NotFoundState`
  parity across detail pages — but this is hygiene, not a blocker
  for C (unlike B where it is the blocker).
- **`routeForHit` lifecycle.** Stays — backend still emits
  `Redirect` for singleton, but dropdown row clicks still need the
  helper for NFT composite routing.
- **Branch / scope.** C is non-breaking wire change ⇒ can land
  alongside Phase 3 (since both touch `fetch_search` SQL +
  classifier) or as its own task. Decision at pickup.
- **Rollout sanity check.** Add an integration test that hits
  `/v1/search?q=<full G-strkey for known account>` and asserts
  `type == "redirect"`. Currently zero integration coverage for
  redirect semantics (see queries audit).

**Why deferred from 0270 session:**

User explicitly chose to defer this refactor and ship 0270
minimalist. PR #220 (0270) already landed the canonical-strkey
wire + NFT composite routing + FE ledger redirect; the redirect
collapse is orthogonal architectural cleanup, not a launch
blocker. Reasoning quoted: _"warto, ale nie krytyczne. (…) 1 RTT
mniej, prostszy wire, ale to wire breaking change i wymaga
dedykowanej sesji + manual UI verify każdej kategorii input."_

Note: that quote applied to option B (wire-breaking). Option C
preserves the wire and removes the "manual UI verify every
category" cost — at pickup time, C is materially cheaper than
the original 0270 session estimated for B.

Effort estimate for C: ~2-3h impl (delete `fetch_redirect`,
re-enable `tx_hits`, add singleton synthesis in handler, update
unit + integration tests) + ~30 min detail-page 404 hygiene
audit (optional). No FE work. No wire migration.

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
