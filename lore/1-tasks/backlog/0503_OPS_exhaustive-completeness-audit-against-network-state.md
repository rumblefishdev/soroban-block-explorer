---
id: '0503'
title: 'OPS: exhaustive completeness audit — every indexed entity against real network state'
type: OPS
status: backlog
related_adr: ['0055']
related_tasks: ['0502', '0463', '0321', '0500', '0501', '0492']
tags: [ops, clickhouse, data-integrity, audit, priority-medium, effort-medium]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0463. Every completeness measurement this project has ever
      made was taken over our OWN data, which cannot see what we never
      ingested. The checkpoint snapshot removes that blindness for the first
      time. Blocked on task 0502 (the decoder).
---

# OPS: exhaustive completeness audit

## The blindness this removes

Every completeness check we have ever run sampled **our own tables** and
compared entries we already hold against the chain. That method cannot
detect an entity we never ingested at all — it has no row to sample. With
78.85 % of chain history predating our ledger floor, the size of that blind
spot has never been known.

The checkpoint snapshot (task 0502) is a full state snapshot of pubnet, so
for the first time the question is answerable in the correct direction:
**not "is what we have correct?" but "what does the network have that we do
not?"**

Two live examples of what the old method missed, both found in one day:
~52k merged accounts still holding phantom XLM (task 0321), and dead accounts
rendering as alive (task 0500). Both were found by accident, not by a check
designed to find them.

## Scope — per entity, both directions

For each entity we index, compare our deduplicated state against the
snapshot and report **four** numbers, never a single "match" percentage:

| Entity                    | Our table           | Snapshot entry                            |
| ------------------------- | ------------------- | ----------------------------------------- |
| accounts                  | `accounts`          | `AccountEntry`                            |
| classic + native holdings | `balances`          | `TrustLineEntry` + `AccountEntry.balance` |
| Soroban token holdings    | `balances` (type 3) | `ContractData` balance entries            |
| LP positions              | `lp_positions`      | pool-share `TrustLineEntry`               |
| pools                     | `liquidity_pools`   | `LiquidityPoolEntry`                      |
| contracts                 | `soroban_contracts` | `ContractData` / `ContractCode`           |
| signers + thresholds      | `account_signers`   | `AccountEntry`                            |

Per entity, report:

1. **missing** — in the snapshot, absent from us (the blind spot),
2. **ghosts** — in us, absent from the snapshot (the 0321 class),
3. **divergent** — present in both, values disagree,
4. **stale** — present in both, our `lastModifiedLedgerSeq` is behind.

Report absolute counts **and** the value at stake where the entity carries
one (phantom XLM, mislabelled holders), because a count alone does not convey
whether a gap matters.

## Rules for the audit itself

- **Read-only.** This measures; remediation is a separate task per finding.
- **No Horizon.** Raw XDR only — Horizon is legacy and synthesizes fields the
  ledger does not carry.
- **Report the method with the number.** State what was measured exhaustively
  versus sampled, and the sampling rule where used.
- **Do not fold findings into this task.** Each real gap becomes its own
  task with its own measured scale, the way 0321/0500/0501 did.
- **Make it repeatable.** The value is in re-running it after every backfill
  or write-path change; a one-shot script that nobody can re-run is a failure
  of this task even if the numbers were right.

## Acceptance criteria

- [ ] Four-way counts per entity, with the method stated per number
- [ ] Value-at-stake reported wherever the entity carries value
- [ ] Every non-trivial gap filed as its own task with its measured scale
- [ ] Re-runnable by someone who was not here — documented invocation
- [ ] **Docs updated** — `docs/backfills.md` gains the audit as a procedure
- [ ] **API types** — N/A
