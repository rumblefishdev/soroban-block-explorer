---
id: '0198'
title: 'Canonical SQL 06 Statement B: UNION ALL over partial indexes to avoid Seq Scan on account_balances_current'
type: BUG
status: active
related_adr: ['0026', '0037']
related_tasks: ['0048', '0167', '0172']
tags: [priority-medium, effort-small, layer-backend, performance]
milestone: 2
links:
  - web/audit-0048.md
  - docs/architecture/database-schema/endpoint-queries/06_get_accounts_by_id.sql
  - crates/api/src/accounts/queries.rs
history:
  - date: 2026-05-07
    status: active
    who: FilipDz
    note: 'Spawned from web/audit-0048.md MEDIUM finding. Same shape as 0172 contracts-stats fix, different table.'
---

# Canonical SQL 06 Statement B: UNION ALL over partial indexes

## Summary

Canonical SQL `06_get_accounts_by_id.sql` Statement B filters only on
`WHERE abc.account_id = $1`. The three indexes on `account_balances_current`
are partial — none match a query without an `asset_type` predicate, so
Postgres falls back to **Seq Scan**. Fine in dev (~22k rows, ~1ms);
production blocker at projected ~50M rows.

## Context

Discovered during the second-pass audit of 0048. EXPLAIN against live DB
confirms Seq Scan. Index inventory + analysis in
[`web/audit-0048.md`](../../../web/audit-0048.md) "Balances —
`fetch_balances`". Same fix shape as 0172.

## Implementation

- Rewrite canonical 06 Statement B as `UNION ALL` over two partial-index
  branches (one with `asset_type = 0`, one with `asset_type <> 0`).
- Mirror change in `crates/api/src/accounts/queries.rs::fetch_balances`.
- Verify both branches use partial indexes via EXPLAIN; attach output to PR.
- Existing 12 accounts integration tests should pass unchanged.

## Acceptance Criteria

- [ ] Canonical SQL 06 Statement B rewritten as UNION ALL
- [ ] `fetch_balances` impl matches new canonical SQL
- [ ] EXPLAIN against live DB shows partial indexes used (no Seq Scan)
- [ ] Existing accounts tests still green
