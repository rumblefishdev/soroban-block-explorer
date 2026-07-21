---
id: '0425'
title: 'REFACTOR: delete the spent one-off backfill subcommands + write down the rule that governs the next one'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0404', '0232', '0392', '0309', '0421']
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

- [ ] Delete the 6 spent subcommands agreed in review — `wasm-upgrade-backfill`,
      `upgradeable-backfill`, `nft-reparse`, `soroban-token-flow-backfill`,
      `pool-ids-backfill`, `assets-id-backfill` — including their `Command`
      variants, dispatch arms, and modules. Per the file-deletion policy, `git mv`
      to `.trash/`, never `rm`.
- [ ] `metadata-backfill` is the 7th candidate: rule 2 forbids its shape. Confirm
      with the owner before deleting — it was excluded from the agreed 6 only
      because an earlier draft proposed keeping it as a template.
- [ ] Add `crates/backfill-runner/README.md` carrying the rule above and the
      audit table. Authoring guide — operations stay in `docs/backfills.md`,
      which it links rather than duplicates.
- [ ] Keep `run`, `status`, `bootstrap`, `balance-seed`, `repair-tier1`,
      `nft-reclassify`, `contract-type-rebuild`. Annotate the last three in
      `--help` with the live-gap task that will retire them.
- [ ] Prune whatever the deletions orphan (helpers, imports, `ch_staging` users).

## Acceptance Criteria

- [ ] `backfill-runner --help` lists only commands that are runnable **and**
      still have a reason to exist.
- [ ] `cargo clippy --all-targets -- -D warnings` clean after the prune.
- [ ] `docs/backfills.md` no longer references a deleted subcommand (its
      reference section names all of them today).
- [ ] The README states rules 1–5 and the audit table, and is linked from
      `docs/backfills.md`.
- [ ] Docs updated — `docs/backfills.md` touched; `docs/architecture/**` `N/A`
      (ops tooling, no architecture shape change).
- [ ] API types regenerated — `N/A`: no `crates/api/**` or `Cargo.*` change.
