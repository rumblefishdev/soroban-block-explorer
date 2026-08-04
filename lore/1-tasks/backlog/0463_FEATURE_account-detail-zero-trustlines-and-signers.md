---
id: '0463'
title: 'FEATURE: account detail — show zero-balance trustlines + signers/thresholds (one RPC enrichment)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0321', '0331', '0295', '0214']
tags:
  [
    frontend,
    backend,
    account-detail,
    clickhouse,
    soroban-rpc,
    priority-medium,
    effort-medium,
  ]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/377']
history:
  - date: '2026-08-04'
    status: backlog
    who: karolkow
    note: >
      Triaged from issue #377 (two asks in one report, both on the account
      detail page, both answered by the same Soroban RPC round-trip — kept
      as one task rather than split). Claims verified against prod
      ClickHouse and Horizon before filing; see Verified findings.
---

# FEATURE: account detail — zero-balance trustlines + signers/thresholds

## Summary

Two gaps on the account detail page, reported together and fixed together
because one Soroban RPC `getLedgerEntries` call answers both:

1. **A trustline that exists but holds 0 is invisible.** The API drops it.
   Other explorers show it, correctly — an established trustline at zero is
   a real fact about the account (it can receive that asset), not an absence.
2. **Signers and thresholds are not shown at all.** There is no way to tell
   from our page whether an account is multisig. We do not index this data
   anywhere today.

## Verified findings (2026-08-04, prod ClickHouse + Horizon)

Fixture account (from the report):
`GDXWIA4VF3GW2R5OSVIROD47W6AQHE33DSEG6TF7YZD3DYOVU54MYBEN`

### Balances

Our `balances` table holds five rows for it; the page renders two.

| asset | `amount` | `last_updated_ledger` | shown today |
| ----- | -------- | --------------------- | ----------- |
| XLM   | 59998533 | 62814867              | yes         |
| AQUA  | **0**    | 58469457              | no          |
| KALE  | 11010000 | 62032915              | yes         |
| SHX   | **0**    | 59023860              | no          |
| USDC  | **0**    | 58469453              | no          |

Horizon confirms all three zero rows are **live trustlines** at
`0.0000000`. So the data is indexed correctly and the read path hides it:

```
crates/api/src/accounts/queries.rs:422
    WHERE b.holder_id = ? AND b.amount != 0
```

### The filter is doing double duty — this is the whole difficulty

A **removed** trustline is also written as `amount = 0`
(`crates/db-clickhouse/src/persist/stage.rs:33-40` module docs, write site
at `:1686`). Under `ReplacingMergeTree(last_updated_ledger)` that zero is
the surviving version, which is exactly the intended tombstone. So:

> **live trustline holding zero** and **closed trustline** are byte-identical
> rows today. Deleting the `!= 0` filter would resurrect closed trustlines as
> ghost rows — the same class of wrongness as the merged-account ghosts in
> task 0321, only inverted.

Scale, for context: 41.9M rows carry `amount = 0`, 36.1M of them classic
credit assets. **Raw row count — not deduped current state**; the dedup
aggregate exceeded the 3.73 GiB per-query memory limit. Order of magnitude
only, but it says both the hidden set and the ghost risk are large, not
edge cases.

### Signers / thresholds are not indexed anywhere

- No signer extraction exists in `crates/xdr-parser` (the only `Signer`
  matches are the `RevokeSponsorship` operation variant — unrelated).
- `thresholds` appears once, as a field of the per-operation state-change
  JSON (`crates/xdr-parser/src/ledger_entry_changes.rs:323`), never
  persisted and never served.
- The `accounts` table (`crates/db-clickhouse/schema/init.sql:142`) has no
  such columns.
- The account summary card shows four facts: account id, sequence number,
  first-seen ledger, last-seen ledger.

Ground truth for the fixture: 5 ed25519 signers at weight 1 each,
thresholds low/med/high = 3/3/3. A genuinely multisig account that our page
presents as an ordinary one.

## Decision — enrich at read time, do not index

Both halves are **current-state** questions, and the detail page shows
current state. Soroban RPC `getLedgerEntries` answers both in one
round-trip:

- `LedgerKey::Account` → the full `AccountEntry`, **signers and thresholds
  included**.
- `LedgerKey::Trustline` for the account's zero rows → **present = live
  trustline, absent = closed**. We do not need to enumerate trustlines
  (RPC has no "all trustlines of X" primitive) — ClickHouse already tells
  us which keys to ask about, and it is a small bounded set.

Why not the schema route (a `removed` / `deleted_at_ledger` column on
`balances`): it needs a historical backfill, and unlike task 0321 that
backfill cannot be derived from the database alone —
`operations_appearances` does not store the ChangeTrust `limit`, so there
is no DB-only way to tell which past zeros were closures. That means an S3
XDR re-parse. Not worth it for a fact the RPC hands us for free.

Why not index signers: same reasoning, plus accounts untouched since before
our index floor (ledger 50,457,424) would have no signer data at all, so
indexing would be both expensive and incomplete.

## Prerequisite — lift the RPC client out of `backfill-runner`

Everything needed already exists, in the wrong crate:

| what                                   | where                                            |
| -------------------------------------- | ------------------------------------------------ |
| `getLedgerEntries` JSON-RPC client     | `crates/backfill-runner/src/rpc_snapshot.rs:141` |
| `account_ledger_key(strkey)`           | same file, `:253`                                |
| `trustline_ledger_key(account, asset)` | same file, `:379`                                |
| `decode_account_snapshot`              | same file, `:402`                                |
| `decode_trustline_snapshot`            | same file, `:428`                                |

That module's own docs (task 0214) pre-authorised this: _"the
refactor-to-a-shared-crate is a one-day move if a second consumer
appears"_. This is that second consumer. Move it next to the existing
`crates/enrichment-shared` RPC code (`nft_token_uri` already speaks Soroban
RPC from the API side) and consume it from
`crates/api/src/runtime_enrichment/`, which is the established fail-soft
per-request enrichment pattern (ADR 0029 style: on failure the fields come
back `null` and the page still renders).

`AccountSnapshot` (`rpc_snapshot.rs:519`) is deliberately lean and throws
signers/thresholds/flags away — widen it.

## Implementation plan

### Step 1 — shared RPC crate

Move the `getLedgerEntries` client + key builders + decoders out of
`backfill-runner` into shared code; `backfill-runner` keeps working through
the new home. No behaviour change, tests move with it.

### Step 2 — widen `AccountSnapshot`

Carry `signers` (key, weight, type), `thresholds` (master weight + low /
med / high), and `flags` off the decoded `AccountEntry`.

### Step 3 — account-detail enrichment fetcher

New `runtime_enrichment` submodule. One batched call per account-detail
request, built from: the account key, plus a trustline key per zero-amount
row ClickHouse returned. Fail-soft — RPC down means the page renders
exactly as it does today, no error, no invented data.

### Step 4 — read path + DTO

- `fetch_balances` drops `AND b.amount != 0` and instead returns the zero
  rows tagged; the enrichment result decides which survive to the wire.
  Closed trustlines are still never shown.
- Account detail DTO gains `signers` + `thresholds`. Regenerate API types.

### Step 5 — frontend

- `AccountBalances`: zero-balance trustlines render as normal rows at
  `0.00`, no special styling beyond the amount itself. The card's
  `N assets` count includes them.
- New signers block on the account page: each signer with weight and type,
  the account's own key marked, plus the three thresholds. When the count
  or weights make the account multisig, say so plainly rather than leaving
  the reader to do the arithmetic.

## Open questions

- Type-3 (Soroban token) holdings sit in the same `balances` table and hit
  the same zero ambiguity, but their existence question is a
  `ContractData` entry, not a trustline. First cut: classic trustlines
  only; decide whether type-3 zeros stay hidden or get the same treatment.
- Whether the enrichment result is worth a short server-side cache —
  measure the added latency before adding one.
- `balance_aggregates_mv` computes `holder_count` as `countIf(amount > 0)`.
  Nothing here changes that, and it must stay that way: a zero-balance
  trustline is not a holder.

## Acceptance criteria

- [ ] A trustline that exists with a zero balance appears on the account
      detail page; the fixture account shows five assets, not two
- [ ] A **closed** trustline still does not appear — verified on an account
      with a known removal, not only on the happy path
- [ ] Signers (key, weight, type) and low/med/high thresholds are shown;
      the fixture account reads as multisig
- [ ] RPC failure degrades to today's behaviour with no error surfaced and
      nothing fabricated
- [ ] The moved RPC client keeps `backfill-runner` green
- [ ] **Docs updated** — `docs/architecture/**` read-path / frontend data
      contract sections, since the account-detail response shape changes
- [ ] **API types regenerated** — yes, the account DTO gains fields
      (`npx nx run @rumblefish/api-types:generate`)
