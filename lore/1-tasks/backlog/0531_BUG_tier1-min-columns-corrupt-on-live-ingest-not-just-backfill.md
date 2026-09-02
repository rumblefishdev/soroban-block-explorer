---
id: '0531'
title: 'Tier-1 MIN columns corrupt on LIVE ingest — fix the storage semantics, then drop every dead column in one window'
type: BUG
status: backlog
related_adr: ['0040', '0044', '0045']
related_tasks: ['0528', '0529', '0322', '0228', '0310']
tags: ['clickhouse', 'data-integrity', 'api', 'indexer', 'ops', 'effort-large']
links: []
history:
  - date: '2026-09-01'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0528. 0528 fixed ONE of six Tier-1 columns; the same
      MIN-under-RMT defect corrupts the other five, and prod sampling confirms
      it. Also corrects a premise carried since 0228: the corruption is NOT
      confined to cross-machine parallel backfill — it happens on ordinary live
      ingest, every time a later event lands in a later batch.
  - date: '2026-09-01'
    status: backlog
    who: karolkow
    note: >
      REDIRECTED after measurement, and 0529 folded in. The original plan
      ("apply 0528's read-time derivation to the other five") does NOT survive
      contact with `accounts`: a page-scoped derivation for 50 accounts reads
      116–181 M rows and 2.34–27.3 GiB per page (two independent prod slices).
      That is a hot path, so read-time derivation is out for the big fact
      tables and the fix has to change the STORAGE semantics instead. 0529
      (drop the nfts column) is absorbed here so the production `ALTER` is paid
      once, not twice.
---

# Tier-1 MIN columns corrupt on live ingest — fix storage, then drop the dead columns

## Summary

Six columns hold a MIN-over-history value inside a `ReplacingMergeTree` state
table. RMT keeps the row with the highest version and replaces it **whole**, so
any later event overwrites the historic minimum. This is not a backfill
artifact — it happens on ordinary live ingest.

0528 fixed `nfts.minted_at_ledger` by deriving it at read time. That worked
because `nft_ownership` is 23 k rows. **Measurement shows the same approach is
not affordable for `accounts`**, so this task fixes the storage semantics rather
than papering over them at read time, and then removes every column that becomes
dead — in a single production `ALTER` window.

## The premise this corrects

`repair_tier1.rs` and task 0228 attribute the corruption to cross-machine
parallel backfill: worker N stamps a first-seen value for its own ledger range
with no visibility into earlier ranges, and the post-merge RMT collapse keeps the
latest writer's value.

True but incomplete. A single indexer reproduces it on live ingest: it sees only
the current batch, so a transfer, burn or later appearance carries no historic
minimum and the RMT replace erases whatever was there. Under that reading
`repair-tier1` is not a post-backfill step — it is recurring cleanup for a defect
that never stops producing.

## Measured on prod (2026-09-01)

### Corruption

| Table               | Column                 | Finding                                                                                                                                                  |
| ------------------- | ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `nfts`              | `minted_at_ledger`     | 642 NULL + 1 stored `0` of 13 932 — **served correctly since 0528**                                                                                      |
| `accounts`          | `first_seen_ledger`    | **14 / 400 sampled (3.5%) diverge**, all LATER than the true first appearance → ~570 k of 16.17 M                                                        |
| `soroban_contracts` | `deployed_at_ledger`   | **1 597 / 146 397 diverge (1.1%)**, plus 3 NULL where the value is known                                                                                 |
| `nfts_pending`      | `minted_at_ledger`     | 4 / 277 NULL; **66 / 277 have no Mint row at all**                                                                                                       |
| `lp_positions`      | `first_deposit_ledger` | not measured — `operations_appearances` (6.85 B rows) sorts on `ledger_sequence`, so a per-pool check is itself a full scan. Same defect by construction |
| `soroban_contracts` | `deployer_id`          | shares the rebuild with `deployed_at_ledger`                                                                                                             |

`accounts.first_seen_ledger` is the exposed one: served on the account detail and
list responses, rendered in `AccountSummary.tsx` and `AccountsTable.tsx`.

Sampling note: the `accounts` figure is a 400-row deterministic slice
(`id % 4001 = 7`) joined to `transaction_participants` on its sort key. A census
is not affordable — that table holds 10.73 B rows. Treat 3.5% as an estimate with
a real sample behind it.

### Cost — why 0528's approach does not generalise

Page-scoped derivation, 50 accounts, `WHERE account_id IN (page)` against
`transaction_participants`, two independent prod slices:

| Slice            | Rows read   | Bytes read | Duration |
| ---------------- | ----------- | ---------- | -------- |
| first 50 by id   | 180 883 365 | 2.34 GiB   | 413 ms   |
| `id % 7919 = 13` | 116 778 764 | 27.29 GiB  | 2 708 ms |

Against a 100 GB/hour read quota that is roughly **4–40 page views per hour**
before the quota is gone. The sort key does not save it: the page's ids are
scattered across the whole key space, so the seek touches granules everywhere.

**Conclusion: read-time derivation is viable only where the fact table is small
(`nft_ownership`, `nft_ownership_pending`) and self-contained
(`soroban_contracts`). For `accounts` and `lp_positions` it is out.**

## Approach — per column, by measured cost

### Small / self-contained → read-time derivation (the 0528 pattern)

| Column                                                 | Derivation                                                                                                                         |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| `nfts_pending.minted_at_ledger`                        | `min(ledger_sequence)` over `nft_ownership_pending WHERE event_type = 0`                                                           |
| `soroban_contracts.deployed_at_ledger` + `deployer_id` | `min(wasm_uploaded_at_ledger)` + `argMin(deployer_id, …)` over rows with a non-NULL deployer — self-contained in a 185 k-row table |

### Large fact tables → maintained MIN aggregate

`accounts.first_seen_ledger` and `lp_positions.first_deposit_ledger` need the
minimum to be **maintained on write**, not recomputed on read. The shape that
expresses MIN correctly under merges — which plain RMT cannot — is an
`AggregatingMergeTree` keyed by the entity, fed by a materialised view on the
fact table's inserts:

```
account_first_seen (account_id, first_seen SimpleAggregateFunction(min, Int64))
  ENGINE = AggregatingMergeTree ORDER BY account_id
  ← MV on transaction_participants inserts
```

Read cost becomes a point lookup on a table with one row per entity (16 M rows,
two columns) instead of an aggregate over 10.7 B. The merge itself carries the
MIN semantics, so nothing can clobber it and no repair pass is ever needed again.

**This is the fundamental fix.** It is also the one that retires `repair-tier1`
entirely rather than reducing its scope.

Open sub-questions to settle with a measurement, not an assumption:

- Backfilling the aggregate for existing rows is a one-off full pass over
  10.7 B / 6.85 B rows. Cost and quota impact must be sized before scheduling.
- Whether the MV fires correctly for the backfill-runner's bulk insert path, not
  only for live ingest.
- Whether `SimpleAggregateFunction(min, …)` or a plain `AggregatingMergeTree`
  with `minState`/`minMerge` reads better here.

### Two traps 0528 hit, certain to recur

- `min()` over a non-Nullable column returns a **non-Nullable** type, and without
  `join_use_nulls = 1` (unavailable — `api_reader` is readonly) a LEFT JOIN miss
  fills the type DEFAULT rather than NULL. Wrap in `nullIf(_, 0)`, or the
  endpoint 500s on decode and a missing value renders as "ledger 0". This is not
  hypothetical: `nfts` held a literal stored `0` that the old code displayed as
  "Minted at ledger 0".
- Where the value is a sort key or cursor key, the ORDER BY, the keyset predicate
  and the cursor payload must move together, or pagination stops being total.

## Absorbed from 0529 — drop every dead column in ONE window

Once nothing reads the six Tier-1 columns, they and the four long-vestigial
metadata columns can go. Paying the `ALTER` window once is the whole reason 0529
was folded in here.

- Strip the fields from the row structs (`db-clickhouse/src/persist/rows.rs`) and
  from the staging merge (`persist/stage.rs`) — including the in-batch `min()`
  folds, which become dead with them.
- Delete the corresponding halves of `repair_tier1`; retire the subcommand once
  nothing it repairs is read.
- Remove the columns from `db-clickhouse/schema/init.sql`.
- **One** production `ALTER` dropping: `nfts.minted_at_ledger`,
  `nfts_pending.minted_at_ledger`, `accounts.first_seen_ledger`,
  `lp_positions.first_deposit_ledger`,
  `soroban_contracts.{deployer_id, deployed_at_ledger}` _(only those actually
  superseded — decide per column)_, plus the already-vestigial
  `nfts.{name, media_url, collection_name}` and the `nfts_pending` twins.

### Deploy ordering — the real hazard

0310 cost ~9 minutes of ingest stall here. The `clickhouse` 0.15 driver validates
the row struct against `DESCRIBE TABLE` **in both directions**:

- slimmed struct + column still present (and no `DEFAULT`) → client-side
  `SchemaMismatch`, inserts fail
- struct still carrying the field + column already dropped → also a mismatch

Neither "deploy first" nor "ALTER first" is safe alone, and warm Lambda
containers cache the `DESCRIBE` until a config-touch recycles them.

**Untested idea worth trying before booking the window:** give each column an
explicit `DEFAULT` first
(`ALTER TABLE … MODIFY COLUMN … DEFAULT NULL`). 0310's failure was specifically
_"table columns without DEFAULT not covered by the struct"_, so a defaulted
column may let the slimmed struct pass while the column still exists — collapsing
the window to zero. **Verify against the driver before relying on it.**

## Ordered steps

1. Deploy 0528 so the NFT fix reaches users. **Not a blocker for steps 2–4** —
   it only gates step 5, because the `ALTER` must follow the deploy of code that
   stopped reading.
2. Read-time derivation for `nfts_pending` and `soroban_contracts` (cheap, the
   0528 pattern).
3. Maintained MIN aggregate for `accounts`, then `lp_positions`: build the MV,
   backfill it, measure, then switch the read.
4. Strip the writer: fields out of the row structs, the staging merge, the schema
   and `repair_tier1`.
5. One production `ALTER` window dropping every dead column, ordered per the
   hazard above; verify ingest health and no ledger gap.
6. Retire the `repair-tier1` subcommands that no longer repair anything.
7. Correct the documentation premise: `repair_tier1.rs` header, task 0228, and
   0322 item (G) all still say "parallel backfill".

## Acceptance Criteria

- [ ] Every one of the six columns is either served from a correct source, or has
      a written decision recording why not and what replaces it
- [ ] `accounts` list and detail measured before/after — no page-latency or
      read-quota regression on the hot path
- [ ] The maintained aggregate proven correct against a fact-table sample, and
      proven to fire for the bulk-insert path as well as live ingest
- [ ] Keyset pagination proven total wherever a corrected value is a cursor key
- [ ] Prod measurement before and after, per column, including a re-measure of
      the `accounts` 400-row sample
- [ ] One `ALTER` window, ingest verified healthy afterwards — no
      `SchemaMismatch`, no ledger gap
- [ ] `repair-tier1` retired or reduced to what it still owns
- [ ] `repair_tier1.rs`, task 0228 and 0322 item (G) corrected
- [ ] **Docs updated** — endpoint-query files for every touched endpoint, and the
      schema docs for the dropped columns
- [ ] **API types regenerated** — expected `N/A` (sources change, wire shape does
      not); confirm with an actual empty diff

## Known limits of the derivation itself

A MIN over a fact table can only be as complete as the facts. Measured on prod:
one `nfts` token and 66 `nfts_pending` tokens exist with **no Mint row at all**.
Investigated for the `nfts` case (contract `CCIP47L5…NWNH`, token 2): 219 events,
17 `mint` events covering tokens 0 and 3–18, none for tokens 1–2, across **both**
event encodings the contract used (map-shaped and scalar-shaped) — and the
parser dropped nothing, 17 events producing 17 rows. The contract was deployed at
ledger 60 908 576, far above our floor of 50 457 424, so coverage is not the
cause: it simply never emitted a creation event for those tokens.

Serving NULL there is correct and must stay correct — an invented `0` is worse
than an honest blank. Do not "fix" these by falling back to the earliest
ownership row; that would report a transfer ledger as a mint ledger.

Note for anyone verifying such a case against the chain: `owner_of` on that
contract now fails with contract error #200 for **every** token, including ones
with recent activity, which is consistent with Soroban state archival rather than
with the tokens not existing. Contract error codes are not a reliable oracle for
"this token was burned".

## Notes

- Do not treat this as urgent-and-total. `accounts` carries the real user
  exposure and the real cost risk; the small tables are nearly free. Sequence by
  exposure, and measure before committing to an approach on the two big fact
  tables.
- The production `ALTER` and the deploy are the operator's to run, not the
  agent's.
