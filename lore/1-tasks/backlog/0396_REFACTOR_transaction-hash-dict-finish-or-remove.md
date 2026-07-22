---
id: '0396'
title: 'REFACTOR: resolve transaction_hash_dict redundancy — finish (Rust→dictGet) or remove (dead-but-prod-wired)'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0395', '0397']
tags: [clickhouse, tech-debt, effort-small, priority-low]
milestone: 3
links: []
history:
  - date: 2026-07-14
    status: backlog
    who: karolkow
    note: >
      Spawned from 0387 deep-dive. transaction_hash_dict is defined + prod-wired
      but never called by crates/api (Rust reads transaction_hash_index directly).
      Redundant, but NOT trivial dead code — decide finish vs remove.
---

# REFACTOR: transaction_hash_dict — finish or remove

## Summary

`transaction_hash_dict` (hash → ledger_sequence, `COMPLEX_KEY_CACHE`) is
**defined and prod-wired but never called by `crates/api`**. The Rust
by-hash path (`lookup_hash_ledger`, `transactions/queries.rs:708`) reads
`transaction_hash_index` directly (PK seek). So the dict is redundant with the
index — but it is NOT trivial dead code: it has a broad footprint.

## Context

Emerged in 0387. Measured: a single by-hash lookup on `transaction_hash_index`
reads **~244k rows** (185 parts) — a dict `dictGet` would make it 0. So the dict
is a real (modest) optimization the read path never adopted, not junk.

Footprint (removal is multi-file, incl. prod ops):

- `crates/db-clickhouse/schema/init.sql` — `CREATE DICTIONARY`
- `scripts/merge-attach-hetzner.sh` — `SYSTEM RELOAD DICTIONARY` in ops flow
- `docs/architecture/.../03_get_transactions_by_hash.sql` — canonical spec, dict = "Hot path"
- `docker-compose.yml` + `crates/db-clickhouse/users.d/dict.xml` — `dict_reader` user
- `docs/architecture/security/clickhouse-rbac.md`, `.../README.md`, `clickhouse-pilot.md`, `22_get_search.sql`
- `docs/runbooks/0228_phase6_validation.md` — PASS criterion "dict LOADED"
- `crates/db-clickhouse/tests/smoke.rs` — exercises it

## Decision (pick one, then implement)

- **Finish (recommended):** point `lookup_hash_ledger` (+ search by-hash) at
  `dictGet('transaction_hash_dict', ...)`. Index stays as the dict SOURCE. Wins
  the 244k→0 by-hash lookup the spec already promised. Cheapest; matches design.
- **Remove:** drop the `CREATE DICTIONARY`, `dict_reader` user (`mv` dict.xml to
  `.trash/`), the smoke-test section, the ops-script reload, and reconcile all
  docs + the runbook criterion + canonical spec. Prod `DROP DICTIONARY` is a
  separate ops/deploy step. Loses the by-hash O(1) path.

## Acceptance Criteria

- [ ] Decision recorded (finish vs remove) with rationale.
- [ ] If finish: `lookup_hash_ledger` uses `dictGet`; smoke test still green;
      measured 244k→~0 on the by-hash lookup.
- [ ] If remove: all listed references reconciled (incl. ops script + docs +
      runbook); prod `DROP DICTIONARY` handed to ops; docs/architecture updated
      per ADR 0032.
