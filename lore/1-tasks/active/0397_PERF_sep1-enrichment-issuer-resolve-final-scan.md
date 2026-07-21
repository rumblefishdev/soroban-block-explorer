---
id: '0397'
title: 'PERF: sep1 enrichment issuer resolve does `accounts FINAL WHERE id=?` — 24M rows/call, 4.5B/6h (one-line fix to bloom seek)'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0387']
tags: [perf, clickhouse, enrichment, effort-small, priority-medium]
milestone: 3
links: []
history:
  - date: 2026-07-14
    status: backlog
    who: karolkow
    note: >
      Found during 0387 read-path priority profiling of prod query_log. NOT an
      API endpoint — a background enrichment worker. Same account-resolve
      antipattern (FINAL scan) as 0387's txlist, different code path.
---

# PERF: sep1 enrichment issuer resolve — `accounts FINAL WHERE id=?`

## Summary

The SEP-1 enrichment worker resolves an asset issuer's `account_id` +
`home_domain` (to fetch its `stellar.toml`) with a **`FINAL` scan** of the
`accounts` table for a single id:

```rust
// crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs:71
"SELECT nullIf(account_id,'') AS issuer_strkey, nullIf(home_domain,'') AS home_domain
 FROM accounts FINAL WHERE id = ? LIMIT 1"
```

`FINAL` forces a full read-time merge of the whole ~37M-physical-row `accounts`
table for ONE id. Measured on prod (`system.query_log`, 6h): **avg 24.2M
read_rows/call, p99 46.7M, p99 2.1s, 187 calls, ~4.52 BILLION rows total.**
This is the single heaviest `accounts`-resolve in the cluster — dwarfing the
0387 txlist resolve (~5M/6h). Background (not user-facing latency), but a large
cluster load competing with the read path.

## Context

NOT an API endpoint and NOT part of 0387 — a background enrichment worker.
Surfaced during 0387's priority profiling. Shares the account-resolve-via-scan
antipattern; if a shared surrogate→StrKey dictionary lands (0387 option C), this
worker should ride it too.

## Implementation

- Replace `FROM accounts FINAL WHERE id = ?` with the codebase-standard bloom
  seek: `FROM accounts WHERE id IN (?) ORDER BY last_seen_ledger DESC LIMIT 1`
  (same semantics — latest version, `home_domain` is mutable so keep the DESC
  ordering; no `FINAL`). Expected **24M → ~1M rows/call (~24×)**.
- OR, if the shared `id→account_id`/`home_domain` dictionary (0387 C) exists,
  `dictGet` → 0 rows. (Note: `home_domain` is mutable, so a dict attribute for
  it needs the dict's refresh/staleness handled; `account_id` is immutable.)
- Verify no behavior change (same issuer StrKey + home_domain resolved).

## Acceptance Criteria

- [ ] `sep1_assets.rs` issuer resolve no longer uses `accounts FINAL WHERE id=?`.
- [ ] Measured read_rows/call drops from ~24M toward ~1M (bloom) or ~0 (dict),
      verified via `system.query_log`.
- [ ] Enrichment output unchanged (issuer StrKey + home_domain identical).
