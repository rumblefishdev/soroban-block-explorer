---
id: '0640'
title: 'BUG: backfill-runner never applies contract upgrades — 18,136 testnet contracts show pre-upgrade code'
type: BUG
status: backlog
related_adr: ['0052']
related_tasks: ['0320', '0425', '0548', '0553', '0564', '0316', '0325', '0639']
tags:
  [
    backend,
    clickhouse,
    data-quality,
    soroban,
    testnet,
    priority-high,
    effort-medium,
  ]
links: []
history:
  - date: '2026-10-14'
    status: backlog
    who: karolkow
    note: >
      Found on testnet by the 0320 invariant: 18,136 of 19,287 upgraded
      contracts stale, mainnet 0. Cause traced to the backfill path; repair
      statement prepared and tested locally, waiting for the owner's run.
---

# backfill-runner never applies contract upgrades

## Summary

`soroban_contracts.wasm_hash` (and the CAP-85 `executable_owner_id` /
`executable_tag`) must be the contract's current executable. Only the live
indexer moves it on an `executable_update` event. `backfill-runner run` writes
the deploy row and drops every upgrade, by design, and nothing repairs it since
`wasm-upgrade-backfill` was deleted in task 0425. Testnet was built by a
backfill after that deletion, so 18,136 contracts show the code they were
deployed with. The same happens to any upgrade inside a range `run` re-ingests
on mainnet (a restore gap, `docs/backups.md`) and on every testnet reset.

## Stan teraz

- Done: repair run on testnet 2026-10-14 (18,136 rows). Afterwards the stored
  executable matches the chain for all 19,287 upgraded contracts.
- Found: the same backfill gap left testnet's classification and NFTs
  unrepaired (`contract-type-rebuild`, `nft-reclassify` never run), see notes.
- Next: the owner picks the code fix (see "Not yet decided") and whether to run
  the two passes on testnet.
- In force: the docs require the repair select after every `run` until the code
  fix lands (`docs/backfills.md`, "After any backfill").

## Cause

1. The live writer reads the contract's current row before it writes the
   upgrade, because `soroban_contracts` is a whole-row ReplacingMergeTree and a
   row without the identity columns would erase them
   (`crates/db-clickhouse/src/persist.rs:571`, `fetch_prior_contract_rows`).
   Without that row it skips the upgrade
   (`crates/db-clickhouse/src/persist/stage.rs:453`).
2. `backfill-runner run` passes an empty map instead
   (`crates/backfill-runner/src/sink.rs:261-265`), so every upgrade in a
   backfilled range is skipped. The comment there says so, and that nothing
   recovers it.
3. Task 0425 deleted the recovery pass on the grounds that "the live indexer now
   does it itself" (`docs/backfills.md`, `backfill-runner` reference). True for
   live data. A history `run` into a fresh database (testnet, 2026-10-05) is not
   covered.
4. The gap was found on 2026-09-16 (0548 `notes/R-task-record.md`, "The sink
   gap, measured"). It reached only `docs/backups.md` as a warning. No task was
   opened, and neither `docs/backfills.md` nor the testnet runbook mentioned it
   when testnet was backfilled.
5. The invariant that would have caught it exists only as a file in 0320's notes
   and covers mainnet only (runner: task 0564).

Not the cause: ordering between parallel workers. The map is empty for
every worker, so a single sequential `run` loses upgrades too.

## Measured

Full numbers, the chain check and the local test:
[notes/R-measurements-2026-10-14.md](notes/R-measurements-2026-10-14.md).

- Testnet: no contract whose last upgrade was backfilled has an upgrade row
  (0 of 18,930). Every live-indexed one has it (357 of 357). 794 of the
  backfilled ones are correct only because their latest code equals the deploy
  code.
- Mainnet (`default`): 0 rows to repair, CAP-85 references included.
- An inline prefetch in `run` would miss 78.7% of testnet upgrades and 7.9% of
  mainnet ones. The deploy sits in the same 64k partition, whose inserts are not
  visible until the partition commits.

## What a user sees

On the 18,136 testnet contract pages: the deploy-time hash shown as current, its
ABI, a decompilation of the old binary, and the "No self-upgrade" chip from the
old imports (0564 lists the queries). The Verified Code tab (0639) would look
up source for the wrong hash.

## Implementation

- [x] **Repair testnet. The owner runs it.** Done 2026-10-14, 18,136 rows.
      [notes/G-executable-repair-select.sql](notes/G-executable-repair-select.sql)
      (read-only, expect 18,136), then
      [notes/G-executable-repair-insert.sql](notes/G-executable-repair-insert.sql)
      (`chw`), then the select again (expect 0). The indexer may keep running.
- [ ] **Code fix**: deferred to task 0641 (2026-10-14), which walks every
      post-backfill pass first. Until then the manual step in the docs holds.
- [ ] Re-check `contract_type` of the repaired contracts. The repair carries the
      class forward, like the live path. Reclassification on upgrade is 0325.
- [ ] The invariant runner of 0564 covers both databases, with this task's select
      as the invariant (it covers CAP-85 references; 0320's file does not).

## Decisions so far

- 2026-10-14: the code fix waits for task 0641's walk over all
  post-backfill passes. Options weighed: executables as their own versioned
  facts (no writer reads a prior row), a two-phase `run`, sequential `run`
  with reads (loses parallelism), or a subcommand run after `run`. A prefetch
  inside today's `run` is ruled out by the 78.7% measurement.

## Acceptance Criteria

- [ ] `G-executable-repair-select.sql` returns 0 rows on `testnet` and `default`.
- [ ] A backfilled range with an upgrade ends with the upgraded executable, with
      no manual step (test on the local ClickHouse).
- [ ] `docs/backfills.md` and `docs/runbooks/testnet-reset.md` match the fix that
      ships. The interim manual step is removed once it is automatic.
- [ ] **Docs updated.** `docs/backfills.md`: updated (interim step, 2026-10-14).
      `docs/runbooks/testnet-reset.md`: updated (2026-10-14).
      `docs/architecture/**`: decided with the code fix (A changes the schema).

## Out

- Reclassification on upgrade: 0325. The RMT whole-row clobber class: 0316.
