---
id: '0425'
title: 'REFACTOR: delete the spent one-off backfill subcommands + write down the rule that governs the next one'
type: REFACTOR
status: completed
related_adr: []
related_tasks: ['0404', '0232', '0392', '0309', '0421', '0426', '0429']
tags:
  [priority-medium, effort-small, backfill-runner, clickhouse, robustness, docs]
links:
  - crates/backfill-runner/src/main.rs
  - docs/backfills.md
history:
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0404 review. `backfill-runner` carries 12 subcommands + 2
      separate bins; most are spent one-shots whose logic the live indexer now does
      itself, but nothing says so, so every one of them still shows up in `--help` as
      if it were an available tool. Audited all 14 against the live path before
      proposing any deletion — the audit table below is the deliverable that matters,
      the deletions are its consequence.
  - date: 2026-07-21
    status: active
    who: karolkow
    note: 'Activated to execute the 6 agreed deletions + the authoring README.'
  - date: 2026-07-21
    status: done
    who: karolkow
    note: >
      Merged as PR #357 (`88a6c295`), 7 commits, 17 files, +167 / -2832. All
      **seven** spent subcommands removed (the 7th, `metadata-backfill`, was
      confirmed and deleted rather than kept as a template), plus the
      `scripts/0266/` shell wrappers. `crates/backfill-runner/README.md` added
      (103 lines) carrying the authoring rule + audit table; `docs/backfills.md`
      rewritten to link it. `--help` now lists exactly the 7 keepers; `cargo
      clippy -p backfill-runner --all-targets -- -D warnings` clean. No tests
      touched. Two findings emerged from measurement and changed the deliverable:
      `bootstrap` reclassified from one-off to **recurring mop** (61.7% of
      transacting accounts carry `sequence_number = 0`), and rule 4 of
      `docs/backfills.md` — the "`--reindex` is unsafe on version-less RMT"
      claim — was **refuted on CH 26.3** and rewritten (spawned 0426, itself
      closed the same day by that measurement).
---

# REFACTOR: delete the spent one-off backfill subcommands

## Summary

Seven `backfill-runner` subcommands exist only because the live indexer did not
yet do what they do. It does now — verified per-command against the live write
path. They are finished tools presented as live ones. Delete them (git keeps
them), and write down the rule that decides whether the _next_ one should exist
at all.

Three commands are **not** spent, and that is the real finding: they are mops
under running taps, and each one marks a hole in live ingest.

## Audit — script vs live forward indexing

### A. Live does the same thing 1:1 — spent, delete

| Subcommand                           | What live does instead                                                           | Evidence                                                          |
| ------------------------------------ | -------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| `wasm-upgrade-backfill` (0320)       | `build_wasm_upgrade_rows` rewrites `wasm_hash` off the `executable_update` event | `persist/stage.rs:249-262`, covered in `tests_cross.rs`           |
| `upgradeable-backfill` (0327)        | parser writes `metadata.upgradeable` on every new WASM                           | module docstring                                                  |
| `nft-reparse` (0296)                 | fixed `detect_nft_events` in the parser                                          | the script **calls that same fn** over `soroban_events`           |
| `soroban-token-flow-backfill` (0383) | `stage.rs` hook registers token-event participants + SAC asset presence          | module docstring                                                  |
| `pool-ids-backfill` (0266)           | `pool_ids` + `gross_volume_a` computed live                                      | `stage.rs:1072`, `stage.rs:925`                                   |
| `assets-id-backfill` (0331)          | `AssetRow::staged` computes `id` with the **same Rust fn** (`ids::asset_id`)     | module docstring; and the `id = 0` population is permanently gone |
| `metadata-backfill` (0304)           | parser writes `soroban_contract_metadata` since 0297                             | hands ledgers to the same `PartitionWriter`                       |

### B. Live cannot express it — not a gap, keep

| Subcommand            | Why live will never cover it                                                                                                                                                                                                              |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `balance-seed` (0331) | live writes a balance only when it **observes** a `ContractData Balance(Address)` change. A holder who has not moved tokens since the parser shipped is never observed. This is an RPC snapshot of current state, not a replay of events. |
| `bootstrap` (0214)    | same shape for `accounts`; and it is a step of `run`, not a standalone script                                                                                                                                                             |

### C. Live is missing it — recurring mop, needs a live fix

| Subcommand                       | The hole                                                                                                                                                                                                     | Owner                                                                                                                                                                            |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **`repair-tier1`** (0228)        | `ReplacingMergeTree` cannot express MIN, so the 6 Tier-1 columns re-drift under live ingest. Measured: `first_seen_ledger` wrong for **97.7% of accounts** on 2026-07-21, five days after a green repair run | [0232](0232_FEATURE_clickhouse-tier1-live-mode-mitigation.md) + [0421](0421_BUG_first-seen-ledger-clobbered-on-every-account-write.md) — engine change to `AggregatingMergeTree` |
| **`nft-reclassify`** (0118/0217) | no continuous `pending → hot` promotion. Measured: hot frozen **33 days**, live adds ~6,575 pending rows/day                                                                                                 | [0392](../active/0392_BUG_nft-pending-live-routing-reconcile/README.md) — **active**                                                                                             |
| `contract-type-rebuild` (0283)   | **partly covered** — live has G1 (verdict by `wasm_hash`) and G9 (by `contract_id`), `stage.rs:192-196`. Contracts the classifier cannot name still default to `Other`                                       | [0309](0309_RESEARCH_parser-classification-design)                                                                                                                               |

## The rule (goes in the README)

1. **Signal already in ClickHouse** → a one-off in-DB pass is allowed (or plain
   `INSERT … SELECT`, per `docs/backfills.md`).
2. **Signal only in XDR** → **no bespoke script.** Re-parse the range with
   `run --reindex`. A targeted-write re-parse binary is a third copy of the
   ingest path — `metadata-backfill` and `pool-ids-backfill` are exactly that
   shape and are the reason this clause exists.
3. **Reuse the live code path, never reimplement it.** Every script that calls
   live code (`detect_nft_events`, `ids::asset_id`, `PartitionWriter`) has stayed
   correct. The two that reimplemented their logic in SQL (`repair-tier1`,
   `contract-type-rebuild`) are the ones the 0388 → 0392 → 0394 → 0404 family
   circles. Same disease as 0404: a second copy of something.
4. **If it cannot be written as "replay live logic over old data", live has a
   hole.** Fix live first; the script is then catch-up, not maintenance. This
   clause is what produced section C above — it is a detector, not just a style
   rule.
5. **Delete it once it has run.** Git keeps it. A spent one-shot left in `--help`
   reads as an available tool.

## Implementation

- [x] Delete the 6 spent subcommands agreed in review — `wasm-upgrade-backfill`,
      `upgradeable-backfill`, `nft-reparse`, `soroban-token-flow-backfill`,
      `pool-ids-backfill`, `assets-id-backfill` — including their `Command`
      variants, dispatch arms, and modules. Per the file-deletion policy, `git mv`
      to `.trash/`, never `rm`.
- [x] `metadata-backfill` is the 7th candidate: rule 2 forbids its shape. Confirm
      with the owner before deleting — it was excluded from the agreed 6 only
      because an earlier draft proposed keeping it as a template. **Confirmed and
      deleted** — `src/bin/` no longer exists.
- [x] Add `crates/backfill-runner/README.md` carrying the rule above and the
      audit table. Authoring guide — operations stay in `docs/backfills.md`,
      which it links rather than duplicates.
- [x] Keep `run`, `status`, `bootstrap`, `balance-seed`, `repair-tier1`,
      `nft-reclassify`, `contract-type-rebuild`.
- [x] Prune whatever the deletions orphan (helpers, imports, `ch_staging` users).
      `rpc_snapshot.rs` trimmed (−13); `ch_staging.rs` survives — still used by
      `repair-tier1` / `contract-type-rebuild`.

## Acceptance Criteria

- [x] `backfill-runner --help` lists only commands that are runnable **and**
      still have a reason to exist. Verified: the `Command` enum holds exactly
      `Run`, `Status`, `Bootstrap`, `RepairTier1`, `ContractTypeRebuild`,
      `BalanceSeed`, `NftReclassify` — the 7 keepers, nothing else.
- [x] `cargo clippy --all-targets -- -D warnings` clean after the prune. Verified
      on `-p backfill-runner`, 2026-07-21: `Finished dev profile`, zero warnings.
- [x] `docs/backfills.md` no longer references a deleted subcommand (its
      reference section names all of them today). The per-command reference
      section is gone; the seven names now appear only in a removal note
      (`docs/backfills.md:288-291`) that points at the README.
- [x] The README states rules 1–5 and the audit table, and is linked from
      `docs/backfills.md` (`:294`). **Consolidated to 4 clauses, not 5** — see
      Design Decisions / Emerged #1.
- [x] Docs updated — `docs/backfills.md` touched. `docs/architecture/**` was
      **not** `N/A` after all: `indexing-pipeline-overview.md` (−21) and
      `xdr-parsing/xdr-parsing-overview.md` (−7) named the deleted passes and
      were updated in the same PR, per ADR 0032.
- [x] API types regenerated — `N/A`: no `crates/api/**` or `Cargo.*` change.

## Implementation Notes

Merged as PR #357 (`88a6c295`), branch
`refactor/0425_delete-spent-one-off-backfill-subcommands`, 7 commits,
17 files, **+167 / −2832**.

| Area             | Change                                                                                                                                                                |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Deleted modules  | `assets_id_backfill.rs` (−293), `nft_reparse.rs` (−417), `soroban_token_flow_backfill.rs` (−311), `upgradeable_backfill.rs` (−294), `wasm_upgrade_backfill.rs` (−368) |
| Deleted bins     | `src/bin/metadata-backfill.rs` (−338), `src/bin/pool-ids-backfill.rs` (−473) — `src/bin/` no longer exists                                                            |
| Deleted wrappers | `scripts/0266/sbe-launch.sh`, `sbe-loop.sh`, `sbe-progress.sh`                                                                                                        |
| Dispatch         | `main.rs` −168 (7 `Command` variants + arms + `mod` lines)                                                                                                            |
| Orphan prune     | `rpc_snapshot.rs` −13                                                                                                                                                 |
| New              | `crates/backfill-runner/README.md` +103                                                                                                                               |
| Docs             | `docs/backfills.md` (−72/+…), 2 architecture docs, 1 test-fixture SQL                                                                                                 |

All deleted files landed in `.trash/`, per the repo-wide `rm` ban.

Live coverage was re-verified on prod immediately before each deletion rather
than inferred from the audit table — e.g. `soroban_contract_metadata` carried a
write from 4 ledgers behind the chain tip, `operations_appearances.pool_ids`
from the tip itself.

## Issues Encountered

- **Rule 4 of `docs/backfills.md` was wrong, and the first draft of this task
  copied it.** The rule asserted that re-parsing history with a different parser
  build is unsafe on the version-less `ReplacingMergeTree` tables (stated: 12;
  actual: 15), which would have blocked clause 1's "use `run --reindex`"
  recommendation. Measured on CH 26.3.10.60 (prod version): RMT keeps the **last
  row inserted**, not an arbitrary one, so a re-parse wins in every shape tried —
  after `OPTIMIZE`, across 4 concurrent inserts, and read through `FINAL`.
  Spawned as task 0426, which the same measurement closed the same day.
  The real hazard is narrower and lives in the parser, not the engine: two rows
  for one key inside a _single_ insert (lore 0356, pool reserves).
- **`bootstrap` was misfiled in this task's own section B.** The audit listed it
  as "live cannot express it — keep". Measurement says it is a recurring mop:
  the account writer stamps `sequence_number = 0` whenever it has no
  account-state override (`persist/stage.rs:699`) and bumps the RMT version
  (`last_seen_ledger`) in the same write, so the zeroed row wins. **61.7% of
  accounts that sent a transaction** in a recent window carry
  `sequence_number = 0`; skeletons are twice as common among active accounts
  (14.7%) as dormant ones (6.75%). Retire via 0421.
- **No tests were touched.** The deleted passes carried no dedicated test files;
  `tests_cross.rs` covers the _live_ equivalents and was already green.

## Design Decisions

### From Plan

1. **The audit table is the deliverable; the deletions are its consequence.**
   Every one of the 14 subcommands was checked against the live write path
   before any removal was proposed.
2. **`git mv` to `.trash/`, never `rm`** — repo-wide file-deletion policy.
3. **Authoring rule lives in the crate README, operations stay in
   `docs/backfills.md`.** The README links rather than duplicates, so the two
   cannot drift into disagreeing.

### Emerged

4. **Rule list consolidated 5 → 4 clauses.** The planned clause 1 ("signal
   already in ClickHouse → in-DB pass allowed") and clause 2 ("signal only in
   XDR → no bespoke script") stated the same test from two sides and read as a
   ban on subcommands in general. Merged into one clause that forbids the actual
   defect — _re-implementing the ingest path_ — and explicitly blesses a
   subcommand driving SQL over already-ingested data, because `--dry-run`,
   counters and a reviewable diff beat SQL pasted into a prod client at 2am.
5. **`metadata-backfill` deleted, not kept as a template.** It was the clearest
   instance of the forbidden shape (parses every ledger in the range in full,
   discards all but one table's rows, carries its own partition loop + watermark
   - resume logic). Keeping the worst offender as the example to copy was
     self-defeating.
6. **Kept commands are annotated with ⚠ and a retirement owner in the README**,
   not just listed. Four of the seven fail clause 3 — they exist because live
   has a hole — and naming the owning task (0232/0421, 0392, 0309) turns the
   keep-list into a work queue instead of a permanent exemption.
7. **`balance-seed`'s classification is marked provisional in the README.** It
   was not measured the way `bootstrap` was; after `bootstrap` flipped under
   measurement, asserting the same category for `balance-seed` on reasoning
   alone would repeat the mistake. Owned by 0429.
8. **Added the defaulted-write invariant to the README.** A whole-row write that
   fills missing fields with defaults is safe _only if it also carries the
   lowest version_. `soroban_contracts`' stub writer obeys it
   (`wasm_uploaded_at_ledger = 0`); `accounts` breaks it; version-less tables
   (`assets`, `wasm_interface_metadata`) are exposed by default. Emerged from
   the `bootstrap` measurement and generalises 0421/0316.

## Future Work

Nothing new spawned — every thread has an existing owner:

- `bootstrap` retirement → **0421** (`first_seen_ledger` / whole-row clobber)
- `repair-tier1` retirement → **0232** + **0421** (`AggregatingMergeTree`)
- `nft-reclassify` retirement → **0392** (re-scope pending; see its 2026-07-21
  re-measurement — the drain gap is dormant, not gone)
- `contract-type-rebuild` retirement → **0309** / **0317** (classifier)
- `balance-seed` classification → **0429** (unobserved-state seeding)
- rule-4 refutation → **0426**, closed 2026-07-21
