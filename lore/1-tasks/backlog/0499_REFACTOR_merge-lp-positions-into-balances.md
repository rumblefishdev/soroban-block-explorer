---
id: '0499'
title: 'REFACTOR: merge `lp_positions` into `balances` per ADR 0056'
type: REFACTOR
status: backlog
related_adr: ['0056', '0055']
related_tasks: ['0463', '0493', '0496', '0497', '0498', '0126']
tags:
  [
    backend,
    clickhouse,
    xdr-parser,
    api,
    liquidity-pools,
    assets,
    priority-low,
    effort-large,
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/405'
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Filed by the lp-holdings decision session that produced ADR 0056.
      TRIGGER-GATED: do not start until task 0493 is scheduled, issue #405 is
      accepted for delivery, or Soroban-AMM feature work begins. The ADR is
      binding for new design immediately; this migration waits for a consumer.
---

# REFACTOR: merge `lp_positions` into `balances`

The full design, rationale, rejected alternatives and rules live in
[ADR 0056](../../2-adrs/0056_liquidity-position-is-a-holding.md) — implement
from there, not from memory of it. This file only carries the work list and
the gates.

## Work list

1. `TokenAssetType::PoolShare = 4`; wire label decided with 0496
   (Horizon's word is `liquidity_pool_shares`).
2. `ALTER TABLE assets ADD COLUMN pool_id FixedString(32) DEFAULT '' , MODIFY
ORDER BY (asset_type, asset_code, issuer_id, contract_id, pool_id)` —
   one statement, metadata-only; mirror in `init.sql`. Karol runs it.
3. `ids::asset_id` arm for pools; parser emits pool asset rows + position
   rows into `balances` from the same pool-share trustline path task 0463
   already stamps with `closed`.
4. Pool-side companion: refreshable MV → plain MergeTree
   `(asset_id, holder_id)`, carrying the position list and
   `first_deposit_ledger` derived from `operations_appearances` type 22.
   **Measurement gate:** the refresh cost; fallback is the sparse column per
   the ADR. The safety net replacement (test on the companion query + probe)
   is part of this item, not optional.
5. Migrate the 40,728 `lp_positions` rows (identity copy — `Decimal128(7)`
   is the same bytes as scaled `Int128`); backfill pool shares from the 0463
   checkpoint snapshot artifact, versioned on entry ledgers, provenance per 0492.
6. Re-point the pool participants read (task 0126's endpoint) at the
   companion; account page (0493) reads `balances` directly.
7. Exclusions: assets list and search `WHERE asset_type != 4` (search may
   opt in deliberately); note on `asset_sac` / `asset_enrichment`.
8. Rework `repair-tier1`: the `lp_positions` entry is deleted (0497 tracks
   the rest); `EXCHANGE`-staging machinery loses its lp target.
9. Retire `lp_positions` (stop writing, then drop after verification) and
   delete the ~30-line LP persist arm from 0463's writer.

## Measured 2026-08-24 — the gap the merge inherits (deferred from 0463)

The 0463 seed decodes pool-share trustlines into `SnapshotState::pool_shares`
but deliberately does NOT diff them: same ledger entry type, different table on
our side.

| side                          | count      | measured                                     |
| ----------------------------- | ---------- | -------------------------------------------- |
| network live pool shares      | **77,048** | snapshot @ checkpoint 64,010,495, 2026-08-18 |
| our `lp_positions`, positive  | **40,652** | live table, 2026-08-24                       |
| our `lp_positions`, at zero   | 68,079     | live table, 2026-08-24                       |
| our `lp_positions`, all pairs | 108,731    | live table, 2026-08-24                       |

**The two sides are six days and ~100k ledgers apart** — this task's own
parent measured export-vs-checkpoint skew as the DOMINANT lever on correction
volume, so the gap below is an order of magnitude, not a number to seed from.
Re-measure both sides at one checkpoint before item 5 acts on it.

**We are missing roughly 36,000 live positions — around 47%.** The same shape as the 60%
classic-trustline gap, and the same live-zero-vs-closed ambiguity sits on the
68,079 zero rows. Work-list item 5 must therefore fill, not just copy: an
identity copy of 40,652 rows carries the hole forward.

### The forward path is already correct — item 5 is history only (2026-08-26)

Re-measured after the 0463 seed executed, and the ambiguity above is bounded in
time rather than spread across the table.

| check                                                         | result                                        |
| ------------------------------------------------------------- | --------------------------------------------- |
| positions touched AFTER the writer deploy (ledger 64,115,052) | **30 / 30 PRESENT on chain**                  |
| every closure stamp in `lp_positions`                         | 44, all at ledger ≥ 64,115,290                |
| positions touched since the deploy                            | 340, of which 44 closed (13%)                 |
| table today                                                   | 108,766 pairs / 40,646 positive / 68,120 zero |

`stage.rs` stamps `closed_at_ledger: if pos.closed { last } else { 0 }` on the
pool-share arm, and the probe says it is right: no live position reads as
closed, no closed one reads as live, for anything the writer has seen.

An earlier 40-row probe returning 16 PRESENT / 24 ABSENT looked like a dead
lifecycle; split by the deploy ledger it is not. Every ABSENT row predates the
writer, which is exactly the cohort the seed skipped on purpose.

So item 5 does **not** need to repair the writer, and the live-zero-vs-closed
ambiguity applies only to rows untouched since 64,115,052 — not to the whole
68,120. Both facts narrow the fill.

Note for the re-measure item 5 already calls for: the 77,048 figure above comes
from checkpoint **64,010,495** (2026-08-18), while the executed seed pinned
**64,131,263** — ~120k ledgers apart. That seed is finished and its
`manifest.json` is final, so the re-measure now has a fixed artifact to align
both sides on.

### The comparator is nearly free — do it as part of item 5

`snapshot.rs` already holds every pool share deduplicated, first-wins, with its
own `lastModifiedLedgerSeq` and a live flag (`SnapshotState::pool_shares`), and
`verdict()` is generic over the key. What is missing is only the other side: a
`stream_our_rows` variant over `lp_positions` and a second `Report`. Estimated
~100 lines, read-only. Deliberately NOT added to the 0463 branch — that branch
was being trimmed, and the number above is what carried the value.

Once `balances` owns pool shares (this task), the seed compares them with no
extra code at all: they stop being a separate table and fall into the existing
classic path.

## Acceptance criteria

- [ ] A classic pool position renders on the account page from `balances`
      alone, one seek, no glue query
- [ ] Pool participants page serves identical data to before (spot-checked),
      within the accepted refresh staleness
- [ ] `first_deposit_ledger` values match the pre-merge stored ones on a
      sampled set — or the fallback column is in use with the measured reason
      recorded in the ADR
- [ ] Assets list and search show zero pool rows; `total_supply` /
      `holder_count` unchanged for spot-checked non-pool assets
- [ ] The 0463 zero-vs-closed probe extended to pool positions returns clean
- [ ] Pool positions are diffed against the snapshot, not just copied — the
      measured 47% gap is closed and re-measured, never carried forward
- [ ] `repair-tier1` no longer touches lp data; `docs/backfills.md` updated
- [ ] **Docs updated** — schema + read path + frontend contract
- [ ] **API types regenerated** — yes, DTOs change
