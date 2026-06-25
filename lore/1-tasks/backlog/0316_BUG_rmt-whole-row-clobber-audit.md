---
id: '0316'
title: 'BUG: RMT whole-row clobber — correct column values overwritten by NULL/default on partial-row insert'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0295', '0297', '0320']
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
  - date: 2026-06-24
    status: backlog
    who: karolkow
    note: >
      Expanded from 0320 research/decision. Added Phase 0 "is it even worth it?"
      recon as the FIRST step (maybe only 1-2 cases → just keep read-modify-write,
      skip the big engine change). Added CoalescingMergeTree as an engine candidate
      alongside SimpleAggregateFunction. Added: if the engine change is adopted,
      0316 must REMOVE 0320's stop-gap read-first prefetch (the read becomes
      unnecessary). Added redundancy reconnaissance to scope.
---

# BUG: RMT whole-row clobber on partial-row insert

## Summary

ClickHouse `ReplacingMergeTree` updates by **replacing the whole row** with the
highest version per sort key — there is no per-column UPDATE (unlike Postgres).
Our staging emits a **full** row every time an entity is _touched_ (referenced
in a ledger), filling only the columns it knows about that ledger and leaving
the rest at `NULL`/`0`/default. Under RMT, a higher-version row carrying those
defaults **overwrites** a correct value written by an earlier, lower-version row.

The root limitation has two manifestations: **(a) clobber-on-reference** — a
partial touch NULLs an existing value (this task's focus; `accounts.home_domain`
below); and **(b) can't-cheaply-update-one-column** — e.g. revising
`soroban_contracts.wasm_hash` on a WASM upgrade ([[0320]]). Both need the same
read-modify-write remedy. This task is the conceptual home + DB-wide audit; 0320
is the wasm-specific implementation.

## Phase 0 — is it even worth it? (reconnaissance FIRST, gates everything)

**Do this before committing to any big engine change.** The fundamental fixes
below (engine swap, side-tables) are large, with data migration. They are only
worth it if the clobber problem is broad. If it turns out to be just 1–2
columns/cases, the lazy correct answer is to **keep the simple per-write
read-modify-write** (read the affected row(s) at staging, carry forward) and
**not** migrate engines at all.

Recon questions to answer first:

1. **Breadth** — how many RMT tables × conditionally-populated columns actually
   suffer measurable clobber? (Extend the `home_domain` measurement query across
   all candidates.) A handful → stay with read-modify-write; many → engine change
   pays off.
2. **Redundancy recon** — where do we already issue read-before-write fetches
   (`fetch_prior_wasm_verdicts`, `fetch_prior_contract_verdicts`, 0320's upgrade
   prefetch), and could one engine change make a whole class of those reads
   redundant? Quantify the reads we'd delete.
3. **Engine fit + availability** — does our ClickHouse version ship
   `CoalescingMergeTree` (keeps last non-null per column on merge — purpose-built
   for partial updates, no read needed)? If not, can `AggregatingMergeTree` +
   `SimpleAggregateFunction(anyLast/argMax)` achieve the same? Migration cost +
   query-side changes (`FINAL`, aggregate read syntax) vs the reads it removes.
4. **Decision gate** — only if (1)+(2) show broad benefit do we adopt an engine
   change. Otherwise: document "read-modify-write is sufficient", close the
   engine question, done.

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
4. **Fundamental fix** (only if Phase 0 says it's worth it) — choose a class-level
   remedy and apply to the affected columns:
   - `CoalescingMergeTree` — keeps the **last non-null per column** on merge, so a
     writer can emit a **partial row (NULLs for unknown columns) without reading
     the existing row first**. The cleanest fit for "update one column"; verify it
     exists in our CH version (Phase 0 q3), or
   - `AggregatingMergeTree` / `SimpleAggregateFunction(anyLast, ...)` — same
     independent-column-merge effect via aggregate columns, or
   - side-table per write-once column (the 0297 pattern), or
   - stop emitting full rows for reference-only touches (only write identity
     columns on a real entity-entry change), or
   - read-modify-write carry-forward at staging (the stop-gap; see below).
5. **Remove 0320's stop-gap read** — 0320 ships a read-first RMW (prefetch the
   upgraded contract's row, carry deployer/name/etc forward) precisely because the
   current RMT engine can't update one column. If 0316 adopts `CoalescingMergeTree`
   / `SimpleAggregateFunction`, that read becomes **unnecessary** — 0316 must rip it
   out (write only the changed column) so we don't keep a redundant DB read forever.
   If Phase 0 says "not worth it", 0320's read-first stays as the permanent answer.

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

| Option                                                           | In-project precedent                                                                                           |
| ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `CoalescingMergeTree` (last-non-null per column, no read needed) | none yet — new engine; verify CH-version availability (Phase 0)                                                |
| `AggregatingMergeTree` + `SimpleAggregateFunction(anyLast)`      | none yet — column-independent merge                                                                            |
| Side-table per write-once column, join at read                   | 0297 `soroban_contract_metadata`                                                                               |
| Fetch-from-DB carry-forward at staging (read-modify-write)       | `fetch_prior_contract_verdicts` (reads prior CH state at staging); 0214/0228 RPC top-up; 0320 upgrade prefetch |
| EXCHANGE one-shot rebuild (`argMax` last-non-null → temp → swap) | 0283 `contract_type_rebuild`, `repair_tier1`, `asset_aggregates`                                               |

Recommendation: a forward-fix (side-table or fetch carry-forward) **plus** a
one-shot EXCHANGE rebuild to repair the existing ~38,833. Read-side
"last-non-null" is rejected — RMT background merges eventually discard the
non-winning versions, so the original value is not recoverable at read.

Note: same read-modify-write infrastructure as the 0320 WASM-upgrade re-classify
(swap one RMT column without clobbering the rest), so the two should share the
staging pattern if the fetch-carry-forward option is chosen.

## Acceptance Criteria

- [ ] **Phase 0 worth-it gate decided**: breadth measured + redundant-read recon +
      CoalescingMergeTree availability → explicit "engine change" vs "keep
      read-modify-write" decision (don't migrate engines for 1–2 cases)
- [ ] Inventory of RMT tables + clobber-candidate columns
- [ ] Per-column measured scale of clobbered rows (with queries)
- [ ] Catalogue of already-fixed cases + the pattern used
- [ ] Architectural decision on the fundamental fix (ADR if warranted)
- [ ] Fix applied to `accounts.home_domain` (+ sequence/first_seen) as the first target
- [ ] If engine change adopted: 0320's stop-gap read-first prefetch removed (write
      partial column only); else: documented that read-modify-write is the permanent answer
