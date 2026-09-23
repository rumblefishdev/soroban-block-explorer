---
id: '0396'
title: 'REFACTOR: resolve transaction_hash_dict redundancy — finish (Rust→dictGet) or remove (dead-but-prod-wired)'
type: REFACTOR
status: active
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
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Measured on prod 2026-07-22 — the remove side of the fork is now clearly
      the right one.**
      `system.dictionaries` confirms the dictionary is real and running:
      `transaction_hash_dict`, status **LOADED**, last successful update
      2026-07-14, **40.01 MiB allocated** — holding **1 element**. A
      `COMPLEX_KEY_CACHE` populating on demand explains the count; it does not
      explain keeping 40 MiB of resident cluster memory for something nothing
      calls. Re-confirmed the "never called" half: **zero `dictGet` occurrences in
      `crates/api`**, so the Rust by-hash path still seeks
      `transaction_hash_index` directly, exactly as the task describes.
      So the dict is not merely redundant with the index — it is redundant *and*
      costs memory on a box where read quotas already matter (see 0250: those
      quotas are not even enforced on the production auth path).
      One caveat against acting immediately: dropping it is a `DROP DICTIONARY`
      on prod, i.e. an ops action, and the task notes a broad footprint
      (definition, prod wiring, possibly init.sql). The code-side prep — deleting
      the definition and any wiring — is safe; the prod drop needs a window.
      Recommendation: remove, not finish. Finishing would mean routing the Rust
      path through `dictGet` to justify 40 MiB, when a PK seek on
      `transaction_hash_index` already answers the same question for free.
  - date: 2026-09-23
    status: active
    who: karolkow
    note: >
      Decided: remove. Production reads NOT_LOADED, 0 elements; task 0580
      re-keys transaction_hash_index by a hash prefix, which the dictionary's
      String key could not follow anyway. PR 1 of task 0580's split.
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

- [x] Decision recorded (finish vs remove) with rationale — remove, whole footprint (below).
- [ ] If finish: `lookup_hash_ledger` uses `dictGet`; smoke test still green;
      measured 244k→~0 on the by-hash lookup.
- [ ] If remove: all listed references reconciled (incl. ops script + docs +
      runbook); prod `DROP DICTIONARY` handed to ops; docs/architecture updated
      per ADR 0032.

## Removal (2026-09-23)

**Decided (karolkow): remove, the whole footprint in one PR** — the
dictionary and its `dict_reader` user, config file and compose mounts, not
the dictionary alone. Production read on 2026-09-23: `NOT_LOADED`,
0 elements, nothing calls it; task 0580 re-keys its source table, which the
dictionary's `String` key could not follow.

Branch `refactor/0396-remove-transaction-hash-dict`, 19 files, +76 / −228:
`CREATE DICTIONARY` out of `init.sql`; `users.d/dict.xml` removed and its
mounts out of `docker-compose.yml` / `docker-compose.prod.yml`; the reload
step out of `scripts/merge-attach-hetzner.sh`; the `dictGet` section out of
the `smoke` test; every doc that named it (`clickhouse-pilot.md` §4e,
`database-schema-overview.md`, canonical SQL 03 and 22 and their README,
`clickhouse-rbac.md` — also its `read_only_lan` profile, which existed only
in that doc —, runbook 0228 step 1.3, both READMEs, `.env.example`). Left as
they were: `docs/scf/milestone-1-evidence.md` and the 2026-05-21 validation
artifact, records of their day.

Verified: `smoke` green on a fresh ClickHouse 26.3 (0 dictionaries after
`init.sql`); both compose files parse, the production one with no
`dict.xml` mount.

**Rollout (the operator's), after the merge:**

1. `DROP DICTIONARY transaction_hash_dict` — before the deploy, because the
   deploy's init sidecar re-applies `init.sql`, and the dictionary must not
   outlive its user.
2. Hetzner `--tags app` from a worktree at the merge: the rsync deletes
   `users.d/dict.xml`; the changed ClickHouse mounts recreate the container
   (three alarms page once and clear in 5–7 min).

Overlaps task 0381's "Dead dictionary + `idx_tx_hash_bloom` removal": the
bloom went in task 0579, the dictionary here.
