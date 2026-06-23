---
id: '0316'
title: 'BUG: RMT whole-row clobber — correct column values overwritten by NULL/default on partial-row insert'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0295', '0297']
tags:
  [layer-data, clickhouse-rmt, data-correctness, priority-medium, effort-medium]
links: []
history:
  - date: 2026-06-23
    status: backlog
    who: karolkow
    note: >
      Spawned during 0295 (AccountMerge balance tombstone) deep dive. Found a
      broader, pre-existing class bug while investigating whether the merge
      tombstone clobbers identity columns.
---

# BUG: RMT whole-row clobber on partial-row insert

## Summary

ClickHouse `ReplacingMergeTree` updates by **replacing the whole row** with the
highest version per sort key — there is no per-column UPDATE (unlike Postgres).
Our staging emits a **full** row every time an entity is _touched_ (referenced
in a ledger), filling only the columns it knows about that ledger and leaving
the rest at `NULL`/`0`/default. Under RMT, a higher-version row carrying those
defaults **overwrites** a correct value written by an earlier, lower-version row.

## Evidence (confirmed on prod CH)

`accounts.home_domain` for the USDC Circle issuer
`GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN`, all RMT versions
(no `FINAL`):

```
ledger 62528000:  circle.com   ← captured correctly
ledger 62976540:  NULL         ← reference-only touch overwrote it
ledger 63039696:  NULL
ledger 63040113:  NULL
ledger 63040312:  NULL         ← FINAL winner (highest last_seen) = NULL
```

`accounts` is `ReplacingMergeTree(last_seen_ledger) ORDER BY (account_id)` with
plain columns (no aggregate, no write-once enforcement — the schema "write-once"
comment is descriptive only). The issuer is referenced in nearly every ledger
(trustlines/payments) → an `accounts` row is staged each time via `account_keys`
(issuer/op-source/participant paths) with `home_domain=None`, `sequence=0`,
`first_seen=current` → RMT promotes the NULL/0 row. `home_domain` is `circle.com`
on-chain (Horizon) but `NULL` in our store.

Likely also affects `accounts.sequence_number` and `accounts.first_seen_ledger`,
and any other RMT table with conditionally-populated columns.

## Why it matters

Identity/metadata columns (home_domain, sequence, first_seen) read back wrong
(NULL/0) for any entity referenced without a full self-update in the winning
ledger — major issuers especially. The API serves these via `... FINAL`, so the
clobbered value is what users see.

## Scope

1. **Inventory** — list every `ReplacingMergeTree` table and its columns; mark
   columns that are populated _conditionally_ (can arrive `NULL`/default on a
   reference-only "touch" insert) → clobber candidates.
2. **Measure** — per table/column, quantify how many entities have a correct
   value overwritten by `NULL`/default (the `home_domain` query above is the
   template: compare non-`FINAL` history vs `FINAL` winner).
3. **Prior art** — find columns already protected against this and learn the
   pattern. Known: task 0297 moved Soroban token metadata to a separate
   `soroban_contract_metadata` side-table precisely to dodge RMT whole-row
   clobber from many writers.
4. **Fundamental fix** — choose a class-level remedy and apply to the affected
   columns:
   - side-table per write-once column (the 0297 pattern), or
   - `AggregatingMergeTree` / `SimpleAggregateFunction(anyLast, ...)` so columns
     merge independently instead of whole-row replace, or
   - stop emitting full rows for reference-only touches (only write identity
     columns on a real entity-entry change), or
   - read-modify-write carry-forward at staging.

## Measured scale (prod CH, 2026-06-23)

`accounts.home_domain`: 1,111,049 accounts had a non-null value in some version;
only 1,072,216 survive in `FINAL` → **~38,833 clobbered to NULL** (3.5%),
concentrated on high-traffic issuers (USDC `GA5ZSEJY…` = circle.com, proven).
Likely mirrored on `sequence_number` / `first_seen_ledger`.

Clobber source is the **participant touch-path** (NOT merge): any account in
`account_keys` (tx source / op source / participant / asset issuer / contract
caller) gets a full `accounts` row staged that ledger; if its own `AccountEntry`
wasn't created/updated that ledger, the row carries `home_domain=None`,
`sequence=0`, `first_seen=current`. Under `RMT(last_seen_ledger)` whole-row
replace, the higher-ledger NULL/0 row wins. Alive, constantly-referenced issuers
are the main victims — the AccountMerge work that spun off this task is unrelated
to the clobber population, it just surfaced the mechanism.

## Fix options — with in-project precedents

| Option                                                           | In-project precedent                                                                    |
| ---------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Side-table per write-once column, join at read                   | 0297 `soroban_contract_metadata`                                                        |
| Fetch-from-DB carry-forward at staging (read-modify-write)       | `fetch_prior_contract_verdicts` (reads prior CH state at staging); 0214/0228 RPC top-up |
| EXCHANGE one-shot rebuild (`argMax` last-non-null → temp → swap) | 0283 `contract_type_rebuild`, `repair_tier1`, `asset_aggregates`                        |

Recommendation: a forward-fix (side-table or fetch carry-forward) **plus** a
one-shot EXCHANGE rebuild to repair the existing ~38,833. Read-side
"last-non-null" is rejected — RMT background merges eventually discard the
non-winning versions, so the original value is not recoverable at read.

Note: same read-modify-write infrastructure as the 0295 bug-1 WASM-upgrade
re-classify (swap one RMT column without clobbering the rest), so the two should
share the staging pattern if the fetch-carry-forward option is chosen.

## Acceptance Criteria

- [ ] Inventory of RMT tables + clobber-candidate columns
- [ ] Per-column measured scale of clobbered rows (with queries)
- [ ] Catalogue of already-fixed cases + the pattern used
- [ ] Architectural decision on the fundamental fix (ADR if warranted)
- [ ] Fix applied to `accounts.home_domain` (+ sequence/first_seen) as the first target
