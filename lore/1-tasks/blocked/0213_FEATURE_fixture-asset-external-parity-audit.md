---
id: '0213'
title: 'FEATURE: fixture-asset external parity audit — per-field correctness vs Horizon + stellar.expert'
type: FEATURE
status: blocked
related_adr: ['0043']
related_tasks:
  ['0188', '0191', '0194', '0195', '0196', '0197', '0199', '0210', '0212']
tags:
  [
    priority-medium,
    effort-medium,
    layer-audit,
    layer-correctness,
    external-parity,
    blocked-on-lp-analytics,
  ]
links:
  - https://developers.stellar.org/docs/data/horizon/api-reference/resources/assets
  - https://stellar.expert/api
history:
  - date: '2026-05-12'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0197 audit-prep discussion. 0197 is Type A (bulk
      volumetric — pipeline-works-on-scale, populated/sentinel/NULL
      ratios). This task is Type B (fixture-based per-field correctness
      vs external source of truth). The two are complementary, not
      substitutes: Type A catches "worker not firing / sentinel emission
      / drain completion" classes; Type B catches "value is wrong /
      drift vs Horizon / parser returns wrong field" classes.
  - date: '2026-05-13'
    status: blocked
    who: karolkow
    by: ['0199']
    note: >
      Promoted backlog → blocked. Scope expanded to cover EVERY audited
      column reaching the API (indexer-driven + enrichment-driven +
      Lambda 2-driven + SQL-computed + runtime type-2), not just
      enrichment-driven. LP analytics columns (`liquidity_pool_snapshots
      .tvl`, `.volume`, `.fee_revenue`) cannot be parity-checked while
      they remain always-NULL (owned by 0199, blocked-on-oracle).
      Running 0213 before 0199 ships would surface LP rows as
      "drift = 100%" with no actionable cause beyond "0199 not shipped"
      — useless noise. Blocking 0213 on 0199 sequences the audit
      correctly: once Lambda 2 populates LP analytics, 0213 runs once
      and covers the full audited surface in one pass.
---

# FEATURE: fixture-asset external parity audit — per-field correctness vs Horizon + stellar.expert

## Summary

Pick a small fixture set of real Stellar entities (assets, LPs, NFT collections),
ingest them locally, run enrichment, and diff each field against external
sources of truth (Horizon, stellar.expert API, issuer's own `stellar.toml`).
Output: `docs/audits/2026-MM-DD-fixture-parity.md` with a per-field
`(ours, horizon, expert, drift, root_cause)` table.

This is the **per-field correctness** half of the audit pair:

- **0197 = Type A (bulk volumetric)** — sample queries on ~hundreds of
  ledgers, ratios of populated / sentinel / NULL per column, status
  command output. Answers: "does the pipeline fire end-to-end at
  realistic scale?"
- **0213 (this task) = Type B (fixture parity)** — manual ingest of
  5-10 specific known entities, per-field comparison with external
  references. Answers: "are the values correct?"

A passing 0197 with a failing 0213 means: pipeline works, but values are
wrong. A passing 0213 with a failing 0197 means: values are correct
where populated, but pipeline doesn't populate at scale. Both must pass.

## Context

### What 0197 cannot catch

Type A samples ratios. It will tell you "30% of NFTs have non-NULL
`media_url`". It will not tell you "the `media_url` we wrote for NFT X
points to a different image than the one Horizon resolves". The
sample-query lens is blind to per-field value drift.

### What 0210 covers (and doesn't)

Task 0210 (BUG: total_supply Horizon parity) does external parity for
**one specific column** (`assets.total_supply`) using a 20-asset sample.
Same shape as this task but scoped to a single MVP correctness issue.
0210 is the special case; 0213 is the generalisation across every
audited column.

### What 0212 covers (and doesn't)

Task 0212 (enrichment live-smoke suite) runs `#[ignore]` tests against
real issuers / collections, asserting **non-NULL** on persisted columns.
It catches "worker silently broke" regressions but does not compare
values to an external source — the assert is `is_some()`, not
`== expected_value`.

### Where this task slots

```
0197 (Type A bulk volumetric) ─┐
                                ├─→ joint coverage: pipeline + values
0213 (Type B fixture parity) ──┘

0210 (total_supply parity) — special case of Type B
0212 (#[ignore] smoke suite) — persistent regression catch, not value parity
```

## Scope

### In

**Fixture set** — final selection during execution, but starting candidates
(refine via spike):

- **Native XLM** (asset, baseline / no-issuer case)
- **USDC** (classic credit with stable `stellar.toml`, real SEP-1 data,
  large `total_supply`, multi-DeFi presence — stresses 0210 too)
- **One AnchorUSD or similar long-running anchor asset** (different issuer,
  different TOML layout)
- **One AMM-listed liquidity pool** carrying a tracked asset (verifies
  LP `reserve_a/b`, `total_shares`, `fee_bps`, AND LP analytics `tvl /
volume / fee_revenue` once 0199 ships)
- **One Soroban NFT collection** with stable IPFS-hosted JSON metadata
  (verifies `nfts.{name, media_url, collection_name}` parser)
- **One SAC-wrapped asset** (verifies SAC handling, `assets.name` parser)

Aim 5-7 entities — enough to cover the variety, small enough to diff
manually in one sitting.

**Audited columns — full coverage map.** Every column reaching the API
response shape, per ADR 0043 §Scope. Grouped by population owner:

| Owner                                                   | Columns                                                                                                                                                                                                                                                                                                                                   | External reference                                                                                                                                                                                                                            |
| ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Indexer (on-chain)**                                  | `assets.{asset_type, code, issuer_id, holder_count, total_supply}`, `accounts.{home_domain, sequence_number, ...}`, `liquidity_pools.{reserve_a, reserve_b, fee_bps, total_shares}`, `soroban_contracts.{contract_id, wasm_hash, is_sac, deployer_id}`, `nfts.{contract_id, token_id, owner_id}`, ledger / transaction / operation fields | Horizon `/assets`, `/accounts/{id}`, `/liquidity_pools/{id}`, `/ledgers/{seq}`, `/transactions/{hash}`. Per-field mapping documented in audit md.                                                                                             |
| **Enrichment Lambda 2 (off-chain via SQS)**             | `assets.{icon_url, name}` (sep1_assets kind), `nfts.{name, media_url, collection_name}` (nft_token_uri kind)                                                                                                                                                                                                                              | Raw `stellar.toml` (issuer `home_domain` → `.well-known/stellar.toml` → `CURRENCIES[].image` / `name`); IPFS gateway resolved metadata JSON. Cross-check with stellar.expert.                                                                 |
| **Enrichment Lambda 2 (LP analytics, blocked-on-0199)** | `liquidity_pool_snapshots.{tvl, volume, fee_revenue}`                                                                                                                                                                                                                                                                                     | Horizon `/liquidity_pools/{id}/trades` for gross volume in asset units; `tvl` requires Oskar's price API or external recomputation (Reflector / stellar.expert "tvl_usd"); `fee_revenue = volume × fee_bps / 10000` derivable from the above. |
| **SQL-computed (in handler / query)**                   | `asset_type_name` (CASE), `is_native` (CASE), `last_modified_ledger` (denormalised), full-text search vectors                                                                                                                                                                                                                             | Horizon `asset_type` discriminator + manual case-mapping.                                                                                                                                                                                     |
| **Runtime type-2 (NOT persisted, served by handler)**   | `assets.{description, home_page}` (SEP-1 runtime fetch); heavy fields E3/E14 via stellar_archive                                                                                                                                                                                                                                          | Raw `stellar.toml` `CURRENCIES[].desc` and `DOCUMENTATION.ORG_URL`.                                                                                                                                                                           |
| **Skip (out of audit scope per ADR 0043)**              | FK helpers, surrogate ids, watermarks (`first_seen_ledger`, `last_updated_ledger`), GENERATED FTS, appearance index tables                                                                                                                                                                                                                | —                                                                                                                                                                                                                                             |

**External references:**

- Horizon `https://horizon.stellar.org/...`
- stellar.expert `https://api.stellar.expert/explorer/public/...`
- Issuer's own `stellar.toml` (raw HTTP fetch via `curl`)
- For NFTs: IPFS gateway resolved value (sanity check that the URL we
  store actually serves the expected image / metadata)

**Per-field diff table** in `docs/audits/{TIMESTAMP}-fixture-parity.md`:

| Entity | Field                 | Our value        | Horizon | stellar.expert   | Raw source   | Drift   | Root cause if drift            |
| ------ | --------------------- | ---------------- | ------- | ---------------- | ------------ | ------- | ------------------------------ |
| USDC   | `assets.icon_url`     | `https://...png` | n/a     | `https://...png` | TOML `image` | match   | —                              |
| USDC   | `assets.total_supply` | `1.2e9`          | `2.0e9` | `1.95e9`         | n/a          | **40%** | see 0210 (SAC + LP not summed) |
| ...    | ...                   | ...              | ...     | ...              | ...          | ...     | ...                            |

**Drift tolerance:**

- String fields (`icon_url`, `name`, `home_page`, `media_url`): exact match
  (modulo URL canonicalisation — `http://` vs `https://`, trailing slash).
- Numeric fields (`total_supply`, `holder_count`): < 1% drift acceptable
  on the value (rounding); anything larger requires root cause.
- Boolean / discriminator fields (`asset_type`, `is_sac`): exact match.

### Out

- Continuous parity monitoring (CI job re-running this audit on a
  schedule) — separate future task. This is a one-shot audit.
- Field-level drift _fix_ work — drift findings spawn follow-up
  tasks (like 0210 was spawned for `total_supply`). This task only
  surfaces drift, doesn't fix it.
- Bulk-scale verification (covered by 0197 Type A).

## Implementation Plan

### Step 1: Fixture selection spike (~30 min)

For each candidate (Native XLM, USDC, AnchorUSD, AMM pool, NFT
collection, SAC asset):

- Verify it has a stable on-chain footprint (last activity within last
  N ledgers).
- Verify it has the data needed (USDC needs SEP-1 TOML; NFT needs
  resolvable IPFS metadata; LP needs both reserves > 0).
- Verify Horizon / stellar.expert return data for it (some explorers
  rate-limit or 404 for niche assets).

Document the final pick + rationale in the audit doc.

### Step 2: Local minimal ingest

For each fixture, identify **the smallest ledger range** that captures
its current state on-chain (typically one recent ledger where the entity
had activity, plus its `last_modified_ledger` if known):

- `backfill-runner` with `--keep-partitions`, smallest possible range
  containing the fixture's activity.
- Verify post-ingest that each fixture's row is present in the local DB.
- Repeat per fixture until all 5-7 are present.

### Step 3: Run enrichment

- `backfill-enrichment-runner sep1-assets` → enriches `assets`.
- `backfill-enrichment-runner nft-metadata` → enriches `nfts`.
- Verify exit code 0 + no error rows in worker logs.

### Step 4: Pull external references

For each fixture, capture raw external responses:

```bash
curl -s "https://horizon.stellar.org/assets?asset_code=USDC&asset_issuer=GA5ZSE..." > /tmp/horizon-usdc.json
curl -s "https://api.stellar.expert/explorer/public/asset/USDC-GA5ZSE..." > /tmp/expert-usdc.json
curl -s "https://centre.io/.well-known/stellar.toml" > /tmp/centre-toml.toml
```

Store raw responses under `notes/sources/` (per lore-framework
sources convention) so the diff is reproducible.

### Step 5: Build the diff table

Per fixture, per audited field, write one row to the audit doc:

- "Our value" — `SELECT field FROM table WHERE id = ?` against local DB
- "Horizon" — parsed value from `/tmp/horizon-*.json`
- "stellar.expert" — parsed value from `/tmp/expert-*.json`
- "Raw source" — TOML / IPFS metadata field (where applicable)
- "Drift" — match / drift % / N/A
- "Root cause if drift" — one-line diagnosis. May reference an existing
  task (e.g. "see 0210") or surface a new bug that spawns its own task.

### Step 6: Spawn follow-up tasks

Each drift row that isn't already tracked spawns a backlog BUG / FEATURE
with `audit-gap` tag, similar to 0210's relationship to 0194.

### Step 7: Reference from 0197

After 0213 finishes, add a footer to `docs/audits/{0197-timestamp}-list-endpoint-completeness.md`:
"Per-field correctness verified separately by 0213's
fixture-parity audit (`docs/audits/{0213-timestamp}-fixture-parity.md`).
Type A here; Type B there."

## Acceptance Criteria

- [ ] Fixture set (5-7 entities) selected and rationalised in audit doc.
- [ ] All fixtures ingested + enriched on local DB.
- [ ] External references pulled and saved under `notes/sources/`.
- [ ] Per-field diff table committed to `docs/audits/{TIMESTAMP}-fixture-parity.md`.
- [ ] Every drift row > tolerance has a root cause and either a
      pre-existing task reference or a new spawned task.
- [ ] 0197 audit doc cross-references this one in a footer.
- [ ] Lore index regenerated.

## Notes

- **Why fixtures and not full bulk.** Per-field manual diff doesn't
  scale — 11 endpoints × ~8 fields × thousands of rows would be
  weeks of work. Fixture sampling gets correctness signal in hours;
  scale signal is 0197's job.
- **Why deferred from 0197.** 0197 README is already large and its
  scope (bulk volumetric audit + docs refresh) is coherent. Bundling
  per-field external parity would double the audit's depth without
  changing the existing matrix shape — cleaner as a sibling task.
- **0210 is a pre-emptive subset.** 0210 spawned independently as a
  BUG because `total_supply` drift was known specific (20-50% on
  USDC, 4 sources, urgent). 0213's USDC row will reference 0210
  rather than re-discover the same drift. Pattern: when 0213
  uncovers a wide-impact drift, spawn a 0210-style specialised task
  for the fix.
- **Relationship to ADR 0043.** Audit does not modify the rule.
  Findings may amend the rule (e.g. discover a new fast-change
  off-chain category requiring path 4, like 0211 is exploring for
  USD price).
- **Why no `#[ignore]` test.** This is one-shot audit, not regression
  catch. 0212 owns regression catch (persistent CI gate, non-NULL
  asserts). 0213 owns one-shot value parity. Different lifetimes.
- **Why blocked on 0199.** Audit scope explicitly includes LP analytics
  columns (`liquidity_pool_snapshots.{tvl, volume, fee_revenue}`),
  which are populated by Lambda 2 in 0199. Running 0213 before 0199
  ships would surface every LP fixture as "drift = 100%" with the
  only actionable cause being "0199 not shipped" — useless noise that
  masks any real drift in other columns. Sequencing the task behind
  0199 ensures one clean pass covers the full audited surface.
  Alternative considered + rejected: run 0213 twice (once now without
  LP analytics, once post-0199 for LP analytics). Doubles operator
  effort, halves the diff doc value, and risks the second pass never
  happening. Single pass post-0199 is the sane sequencing.
- **Unblock criterion.** 0199's Phase 2 (`tvl`, `volume`,
  `fee_revenue` writes from Lambda 2) ships to develop. The Phase 3
  Soroban DEX adapters are NOT a hard prerequisite for 0213 — Classic
  AMM coverage alone gives a meaningful LP fixture (e.g. USDC-XLM).
