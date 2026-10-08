---
id: '0635'
title: "BUG: account_entry_state keeps a closed account's last row as if it were live"
type: BUG
status: backlog
related_adr: ['0055']
related_tasks: ['0629', '0463', '0500']
tags:
  [backend, clickhouse, indexer, data-integrity, priority-low, effort-medium]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/454'
history:
  - date: '2026-10-07'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0629 (PR #643 review): closed accounts keep their
      pre-merge signers and sponsorship counters; every reader must gate on
      `deleted`. Decided: the gate ships now, the table fix is this task.
---

# account_entry_state keeps a closed account's last row as if it were live

## Summary

When an account is closed (`account_merge`), the indexer writes nothing to
`account_entry_state` (`persist/stage.rs`, the `account_removed` gate), so the
table keeps the last row from before the merge — signers, thresholds, flags and
the sponsorship counters of an entry that no longer exists. Make the table say
the account is closed, so no reader has to remember a separate `deleted` check.

## Context

- On chain a closed account has no entry at all: RPC `getLedgerEntries` for
  `GAYYFF…KZPR` returns `entries: []`, while our table holds `0 / 3`
  sponsorship counters for it (checked 2026-10-07).
- CAP-33: a sponsor cannot be merged (`ACCOUNT_MERGE_IS_SPONSOR`), so a closed
  account can carry a stale `num_sponsored`, never a stale `num_sponsoring`.
- Readers that gate on `deleted` today: the Signers card
  (`web/src/pages/accounts/AccountSigners.tsx`) and the account detail API's
  `sponsorship` (`crates/api/src/accounts/handlers.rs`, PR #643). A new reader
  that forgets the gate shows a closed account as live.
- `balances` already solved the same problem with `closed_at_ledger`
  (ADR 0055); the shape to copy.

## Implementation Plan

1. `account_entry_state` gains `closed_at_ledger Int64 DEFAULT 0`; the writer
   stamps it on `account_removed` (whole row: empty signers, zero counters,
   `closed_at_ledger` = the merge ledger) and clears it on re-creation.
2. The checkpoint seed stamps accounts the snapshot lists as deleted and we
   still hold open — the `balances` closure rule, versioned on the checkpoint.
3. Readers drop their own `deleted` gate in favour of the row; the API
   returns `signing` / `sponsorship` as `null` for a closed row.

## Acceptance Criteria

- [ ] A merged account's row reads closed after the next ledger and after the
      seed; a merge-then-recreate reads live (test).
- [ ] `GAYYFF…KZPR` reads closed; a sample of closed accounts matches RPC
      `entries: []`.
- [ ] **Docs updated** — `docs/architecture/database-schema/*`,
      `docs/backfills.md`.
- [ ] **API types regenerated** — if the API shape changes.
