---
title: 'S: Approved parallel-backfill merge plan'
type: synthesis
status: mature
spawned_from: ../README.md
spawns: []
tags: [merge, clickhouse, hetzner, freeze-rsync-attach, multi-machine]
links: []
history:
  - date: '2026-05-15'
    status: mature
    who: stkrolikiewicz
    note: >
      Approved plan, lifted verbatim from a Claude Code planning session.
      Originally drafted at ~/.claude/plans/pull-aktualny-develop-aktualnie-sprightly-pixel.md.
      Captures the 3-way split decision, FREEZE+rsync+ATTACH PART transport,
      Tier-1 repair pass, post-merge validation, and 14 open questions /
      risks for whoever picks up the implementation.
---

# Plan: Parallel Backfill (3-way split) → Merge into Hetzner Production CH

## Context

The Soroban historical backfill (S3 public archive, "2/5 Soroban era" range) is currently being processed on a single laptop, ~30% of the 2/5 range already written to a **local ClickHouse** instance. To reduce wall-clock time, the remaining 3/5 will be backfilled in parallel on a **second machine** (also writing to a local CH). At the end, both local databases must be merged into a dedicated **Hetzner production CH** server, which will be the canonical store.

The risk is _correctness_: `backfill-runner` was designed for a single sequential process. Several CH tables hold state that depends on prior ledgers (balances, sequence numbers, LP positions, NFT ownership, contract reclassification). Naive concurrent ingest into one database — or naive concatenation at merge time — would produce nullable / stale fields and wrong derived values.

This plan defines: (1) how to slice the work so the two local backfills stay coherent, (2) the merge-and-repair pipeline on Hetzner, (3) the validation gates that prove completeness end-to-end.

**Decisions already settled** (from clarification + task 0216 + ADRs 0044/0045):

- **Topology**: each worker writes to a local CH; merge into Hetzner happens at the end. No live dual-write, no mid-stream cutover.
- **Range split**: disjoint contiguous ranges between workers (single writer per S3 partition). N-way (2/3/4 machines for the 3/5 portion) generalizes cleanly — RMT convergence is N-independent.
- **Hetzner CH**: hardware _live as of 2026-05-15_ per task 0216 — Server Auction box (12+ core AMD Ryzen 9, 128 GB DDR4 ECC, 2× 1.92 TB U.2 NVMe in mdadm RAID 1 ext4 → md1 ~1.7 TB usable, Falkenstein, Ubuntu 24.04 LTS). Single-node MergeTree, CH 26.3 in Docker bound to `127.0.0.1` only, behind Caddy on `:443` with mandatory mTLS. API stays on AWS (Lambdas + Galexie). `db-clickhouse-init` sidecar applies `init.sql` idempotently on every boot. Backup: Borg → BX21 Storage Box (BX21 not yet ordered).
- **Transport from workers to Hetzner**: **FREEZE + rsync + ATTACH PART** per ADR 0045. Not `clickhouse-backup` + S3. Workers SSH-rsync `shadow/<N>/` directories straight into per-worker staging dirs on Hetzner; Hetzner attaches parts table-by-table after schema sidecar has run.
- **Repair pass**: full — `OPTIMIZE FINAL`, bootstrap, NFT Phase 3, asset aggregates, `verify-completeness`, sample-compare against Horizon/stellar.expert.

**Concrete ledger ranges** (derived from `docs/runbooks/backfill_soroban_2of5_fresh_machine.md`):

| Worker                                           | Range                          |    Partitions |   ~Ledgers | ~CH disk @ measured density (64.5 GB/M) | Density-multiplier headroom on actual disk                                                                        |
| ------------------------------------------------ | ------------------------------ | ------------: | ---------: | --------------------------------------: | ----------------------------------------------------------------------------------------------------------------- |
| Laptop 1 (2/5, in flight, **measured baseline**) | `[50,457,424 → 55,103,999]`    |  73 (788–860) |  4,646,576 |      ~300 GB end-state (180 GB at 60 %) | On 1 TB SSD: comfortable                                                                                          |
| Machine 2 (mid-3/5)                              | `[55,104,000 → 60,095,999]`    |  78 (861–938) |  4,992,000 |                                  322 GB | **761 GB free**: fits up to ~1.9× of laptop's measured density                                                    |
| Laptop 3 (newest 3/5)                            | `[60,096,000 → L_last_closed]` | ~38 (939–976) | ~2,368,000 |                                  153 GB | **400–500 GB free**: safe at ≤ 2× density (~370 GB peak), marginal at 2.5×, needs split-and-drop fallback at ≥ 3× |

`L_last_closed` = end of the **highest fully-closed S3 partition** at the laptop 3's completion time — not the chain tip. Discovered dynamically by task 0225 sync-validation pre-parse, which refuses to ingest partitions with `<64,000` ledgers in S3.

**What `L_last_closed` looks like in practice.** Chain tip (per stellarchain.io 2026-05-15) is ~`62,577,300`, which falls in partition 977 (`62,528,000–62,591,999`). Partition 977 is _not yet closed on S3_ — it's the live tail. The last fully-closed S3 partition today is therefore partition 976 (`62,464,000–62,527,999`), so `L_last_closed ≈ 62,527,999` if laptop 3 finished right now. By the time laptop 3 actually finishes (~40–60 h after start at its smaller ~38-partition range), the chain will have advanced another ~40–80 k ledgers, closing 0–2 more S3 partitions. Laptop 3 stops at _whatever the highest closed partition is at completion time_ — captured from the audit table, not pinned in the CLI.

This avoids two failure modes:

- Pinning `--end` to a chain-tip number frozen at plan time → underspecified, gap at end.
- Pinning `--end` past the closed-partition frontier → S3 archive lag panics (runbook §9), even with 0225's sync-validation pre-parse (it just stops cleanly there; doesn't extend the archive).

**Live tail** (everything beyond `L_last_closed`) is explicitly out of scope for this batch backfill. The realtime indexer (or a future catch-up pass once those partitions close) handles it. The Hetzner merge boundary equals `L_last_closed` and the production CH advertises completeness only up to that ledger.

Soroban-era partitions through partition 976: `(976 - 788 + 1) = 189` partitions; laptop 1 did 73 in 2/5 (partitions 788–860), machine 2 owns 78 (861–938), laptop 3 owns ~38 (939–976+), plus 0–2 more partitions closing during laptop 3's run. Peak local disk on any worker is dominated by accumulated CH data (S3 scratch never holds more than 2 partitions ≈ 24 GB simultaneously; runner deletes after index unless `--keep-partitions`). Runbook published prereq is 2 TB+; for machine 2's 761 GB free that's already comfortable; laptop 3's 400–500 GB free is the constrained one — see Phase 0 step 5b for the disk-pressure monitoring.

> **Note on `git pull`**: requested at the top of the conversation but skipped here because plan mode forbids non-readonly tools. Run `git fetch origin && git checkout develop && git pull --ff-only` after exiting plan mode. The 0225 branch carried uncommitted edits in `crates/backfill-runner/src/{run,sync,error}.rs` and `Cargo.{lock,toml}` — those belong to task 0225 (sync validation pre-parse), not this merge work. **Task 0225 must land before laptop 3 starts**, because only laptop 3's range reaches close to chain tip where S3 archive lag is an active concern (the exact failure mode 0225 addresses; see runbook §9 troubleshooting). Machine 2's range ends at partition 938 (60,095,999) — far enough from tip to be safe without 0225, but parser-parity still requires the same git SHA across all workers.

---

## Architectural Constraints (must hold for the plan to be sound)

1. **Schema parity**: both local CHs _and_ Hetzner CH must run the _exact same_ schema migration. All three apply schema via the `db-clickhouse-init` sidecar from the same git SHA (ADR 0044 `init.sql`). The sidecar runs on every container boot and is idempotent (`CREATE TABLE IF NOT EXISTS`), so the invariant holds as long as all parties pin to the same SHA. Schema drift = silent corruption. **Required pre-merge fix**: `init.sql`'s `transaction_hash_dict` SOURCE clause hardcodes `USER 'default' PASSWORD 'clickhouse'`; Hetzner uses a different password. Apply the `users.d/dict.xml` localhost-only `dict_reader` user fix (Surfaced trap #2 in task 0216 deployment reference) before the schema sidecar runs on Hetzner.
2. **Parser parity**: all workers (laptop 1, machine 2, laptop 3) must run the _same `backfill-runner` binary hash_, built from the same git SHA on `develop` (or a tagged release). Different parser versions on the same ledger range can produce different rows. Tag the SHA before any new worker starts; record it in the audit table.
3. **Disjoint ledger ranges**: split fixed at `55,103,999 / 55,104,000` (partition 860 / 861 boundary, multiple of 64,000). No partition is touched by both workers.
4. **Partition alignment**: there are two distinct concepts called "partition" and only one needs strict alignment.
   - **S3 source partition** (Stellar public archive, 64,000 ledgers per bucket): both internal splits — `55,103,999 / 55,104,000` (partition 860/861) and `60,095,999 / 60,096,000` (partition 938/939) — are multiples of 64,000. Every S3 partition is processed by exactly one worker. Clean — required for the backfill-runner's single-process-per-partition semantics.
   - **CH `PARTITION BY intDiv(ledger_sequence, 500_000)`** (per-table on-disk layout, independent of S3): with three workers, **two CH partitions straddle worker boundaries**:
     - **CH partition 110** (ledgers `55,000,000 – 55,499,999`) — receives parts from laptop 1 AND machine 2 (split at 55,104,000)
     - **CH partition 120** (ledgers `60,000,000 – 60,499,999`) — receives parts from machine 2 AND laptop 3 (split at 60,096,000)
   - This is **not a correctness issue**. Fact tables have `ORDER BY` keys including `ledger_sequence`, and rows in each straddle are disjoint by ledger → no row-identity overlap → RMT has nothing to dedupe across workers in those partitions, only to merge the parts. State tables (`accounts`, `account_balances_current`, `nfts`, `lp_positions`, `soroban_contracts`, `assets`, `liquidity_pools`) have **no `PARTITION BY`** at all, so straddles don't apply to them.
   - Operational cost: two extra `OPTIMIZE TABLE <fact_table> FINAL PARTITION X` per partitioned table (partitions 110 and 120). Tiny vs. the rest of the pipeline. Strict alignment would require LCM(64_000, 500_000) = 8,000,000 — pushing every split to multiples of 8 M ledgers, which adds significant wall-clock to laptop 1 with no correctness benefit. Rejected in favor of the two straddles.
5. **Idempotency hooks already in the schema**: deterministic `CityHash64` surrogate IDs + `ReplacingMergeTree(version_ledger)` over the natural sort key. The runbook explicitly states "RMT dedup means re-running an overlapping range is a no-op." This means re-inserting the same ledger row is safe, and the row with the highest `version_ledger` wins after `FINAL` / background merge. **The merge plan leans on this — do not break it.**
6. **Task 0225 prerequisite**: laptop 3's range approaches chain tip where S3 archive lag is real. The sync-validation pre-parse work in 0225 (current branch) must land on `develop` before laptop 3 starts (`run --start 60096000 --end <L_last_closed>`).

---

## Sequential-Dependency Danger List

Built bottom-up from `crates/db-clickhouse/schema/init.sql` (19 tables + 1 dictionary, see lines 87–408) plus a write-path audit of `crates/db-clickhouse/src/persist/{stage,rows}.rs`. **Critical confirmed property**: the per-ledger writer never reads CH state during ingest — every column is sourced from the current ledger's XDR (absolute post-state). Bootstrap is the only CH-reader and runs post-ingest. This means RMT version-column convergence on disjoint ranges is correct _by construction_ for any column the parser emits unconditionally per row.

The remaining risk surface is narrow:

### Tier 1 — Cross-machine RMT-overwrite (must repair post-merge)

The pattern: column was written by m1 with a real value at a row whose version-column = `V1`. m2 later wrote a row for the same ORDER BY key at version `V2 > V1`, but m2's parser only had a partial view (e.g., processed a transfer, not a mint) and either emitted `None` for that column or "defaulted to current ledger". RMT FINAL keeps only the row with max version → m1's correct value is lost.

| Table               | Risky column(s)                                            | Where evidenced                                                                                                                                                                                                                                                                                                                                                                           | Repair strategy                                                                                                                                                                                                                                                                                  |
| ------------------- | ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `accounts`          | `first_seen_ledger`                                        | Parser-audit: defaults to `last_seen_ledger` if XDR doesn't emit a prior value. Machine 2 first sees an account that was active in m1's range → emits `first_seen_ledger = current_ledger` (well after the true first appearance). RMT(`last_seen_ledger`) picks m2's row.                                                                                                                | Post-merge: `MIN(ledger_sequence)` from `transaction_participants` / `operations_appearances` per `account_id`. Rebuild via staging table + `EXCHANGE TABLES` (cheaper than `ALTER … UPDATE` mutation at this scale).                                                                            |
| `lp_positions`      | `first_deposit_ledger`                                     | Same defaulting pattern as `accounts.first_seen_ledger` (parser-audit, table row). Risky for pools/accounts whose first deposit predates m2's range.                                                                                                                                                                                                                                      | Post-merge: `MIN(ledger_sequence)` from `operations_appearances` filtered to deposit ops, grouped by `(pool_id, account_id)`.                                                                                                                                                                    |
| `nfts`              | `minted_at_ledger`, `collection_name`, `name`, `media_url` | Verified at `crates/db-clickhouse/src/persist/stage.rs:920-1012`: within a single batch the parser folds via MIN/first-non-NULL, but cross-batch (i.e., cross-machine) it relies on RMT(`current_owner_ledger`). A transfer-only batch on m2 emits `minted_at_ledger=None`, `name=None`, etc.; if `current_owner_ledger` (transfer ledger) > m1's mint row's version, m2's NULL row wins. | Post-merge: rebuild from `nft_ownership` mint events (`event_type = Mint`). Recompute `minted_at_ledger = MIN(ledger_sequence) WHERE event_type = Mint` per `(contract_id, token_id)`; rehydrate metadata from the row's XDR via parser or from the mint event's payload if persisted elsewhere. |
| `nfts_pending`      | same four columns                                          | Same write path as `nfts` (stage.rs:976-1009).                                                                                                                                                                                                                                                                                                                                            | Same repair, then re-run promotion logic to clear quarantine.                                                                                                                                                                                                                                    |
| `soroban_contracts` | `contract_type`, `deployer_id`, `deployed_at_ledger`       | Edge case: deploy in m1, WASM upload in m2 (or vice versa). The WASM-upload row carries `wasm_uploaded_at_ledger` as version; if higher than the deploy row, it wins under RMT but is missing deploy-time fields. Task 0118 Phase 3 reclassification covers `contract_type`; the other two fields are a smaller residual.                                                                 | Run task 0118 Phase 3 SQL. Backfill `deployer_id` / `deployed_at_ledger` from `operations_appearances` filtered to `Create*Contract*` op types per `contract_id`.                                                                                                                                |

### Tier 2 — Plain MergeTree, no version-column dedup (newly identified)

Two tables are `MergeTree`, not `ReplacingMergeTree` — duplicate rows from two machines do **not** automatically collapse:

| Table                     | Risk                                                                                                                                 | Repair                                                                              |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------- |
| `wasm_interface_metadata` | Same `(wasm_hash, metadata)` row inserted by both machines = two persistent rows. Reads need `DISTINCT` or `GROUP BY` to be correct. | `OPTIMIZE TABLE wasm_interface_metadata FINAL DEDUPLICATE BY wasm_hash` post-merge. |
| `ledgers`                 | Disjoint ranges = no overlap in `sequence`, so moot in practice. Defense-in-depth in case of accidental overlap.                     | `OPTIMIZE TABLE ledgers FINAL DEDUPLICATE BY sequence` post-merge — cheap.          |

### Tier 3 — Nullable enrichment, recompute post-merge

These columns are explicitly Nullable in the schema; ingest leaves them NULL; downstream tasks compute them. The merge plan doesn't change their behavior, but the post-merge repair pass must include the recompute.

| Table                      | Columns                        | Owning task                                          |
| -------------------------- | ------------------------------ | ---------------------------------------------------- |
| `assets`                   | `total_supply`, `holder_count` | 0194 (aggregates over `account_balances_current`)    |
| `liquidity_pool_snapshots` | `tvl`, `volume`, `fee_revenue` | 0199 (price oracle + volume/fee derivation; blocked) |

### Tier 4 — Verified SAFE under cross-machine merge

Listed for completeness. **No repair needed**; RMT(`<version_column>`) FINAL is correct by construction because parser always writes absolute XDR post-state.

- `account_balances_current.balance` + `last_updated_ledger` — XDR carries the post-trustline balance per op; m2's row at higher `last_updated_ledger` is the correct latest state.
- `accounts.{sequence_number, last_seen_ledger, home_domain}` — RMT picks latest version; `sequence_number` skeleton-account NULLs are filled by the existing `bootstrap` subcommand (runbook §8a) post-ingest.
- `nfts.{current_owner_id, current_owner_ledger}` — RMT picks latest transfer.
- `lp_positions.{shares, last_updated_ledger}` — absolute post-state per deposit/withdraw op.
- `liquidity_pools.{asset_a_*, asset_b_*, fee_bps, last_updated_ledger}` — pool creation is immutable; parser writes the full asset tuple from XDR on every observation.
- `liquidity_pool_snapshots.{reserve_a, reserve_b, total_shares}` — per-ledger snapshot from XDR, append-only fact.
- `soroban_contracts.{wasm_hash, wasm_uploaded_at_ledger, is_sac, name}` — written when WASM observed; RMT picks the WASM-bearing row.
- All append-only fact tables: `transactions`, `transaction_hash_index`, `operations_appearances`, `transaction_participants`, `soroban_events`, `soroban_invocations_appearances`, `nft_ownership`, `nft_ownership_pending` — every row is per-(ledger, …) and identity-folded only within a single ledger; disjoint range union is correct.

---

## Enrichment Placement: Local vs. Hetzner

Enrichment / repair / derivation passes split into two buckets based on whether they need the full union or can run correctly on a single range.

### Runs PRE-merge, per local CH (cheap, locally correct)

- **`bootstrap` subcommand** (runbook §8a). Fills skeleton accounts (`sequence_number = 0`) via Soroban RPC `getLedgerEntries`. RPC returns _current_ `AccountEntry` state — independent of which ledger range observed the account first. Per-machine pre-merge run handles each range's skeletons; a final pass on Hetzner is cheap insurance.
- **0221 SAC `nfts_pending` drain** (runbook §8b). Mechanical `ALTER TABLE … DELETE` for SAC contracts that leaked into the NFT-candidate quarantine. No cross-machine dependency. Cheaper to drain locally than to ship junk rows to Hetzner.
- **Per-machine baseline metrics** (runbook §8c query). Counts captured into `pre-export-metrics.json` / audit table for the Phase 6 row-count parity check.

### Runs POST-merge on Hetzner, over the full union (correctness requires it)

| Pass                                                                                                                                                                                                                    | Why it can't run local                                                                                                                                         | Source of truth                                                                    |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Tier-1 column rebuilds (`accounts.first_seen_ledger`, `lp_positions.first_deposit_ledger`, `nfts/nfts_pending.{minted_at_ledger,collection_name,name,media_url}`, `soroban_contracts.{deployer_id,deployed_at_ledger}`) | Each needs `MIN(ledger_sequence)` (or equivalent fact-table aggregate) over **both ranges**. Per-machine `MIN` is wrong: m2 has no visibility into m1's range. | `transaction_participants`, `operations_appearances`, `nft_ownership` after merge. |
| Task 0118 Phase 3 NFT reclassification                                                                                                                                                                                  | WASM uploaded in m1's range can reclassify contracts active in m2's range (and vice versa). Reclassification needs all WASM observations.                      | `soroban_contracts` × `wasm_interface_metadata` after merge.                       |
| Task 0194 asset aggregates (`total_supply`, `holder_count`)                                                                                                                                                             | Sum over all trustlines across the union. Per-machine totals miss cross-range holders.                                                                         | `account_balances_current` after merge.                                            |
| Task 0199 LP analytics (`tvl`, `volume`, `fee_revenue`)                                                                                                                                                                 | Volume / fee / TVL aggregate across the union. (Blocked task; in plan only conditionally — runs when 0199 lands.)                                              | `liquidity_pool_snapshots` + `operations_appearances` + price oracle.              |
| Task 0198 Statement B partial-index rebuild                                                                                                                                                                             | One-time index build over the canonical store.                                                                                                                 | `account_balances_current` partitioning post-merge.                                |
| `OPTIMIZE TABLE wasm_interface_metadata FINAL DEDUPLICATE BY wasm_hash` and `OPTIMIZE TABLE ledgers FINAL DEDUPLICATE BY sequence`                                                                                      | Duplicate rows are a merge artifact (both `MergeTree`, no version column). On individual local CHs no duplicates exist.                                        | Hetzner only.                                                                      |
| Per-table per-partition `OPTIMIZE … FINAL`                                                                                                                                                                              | RMT collapse of overlapping rows across machines, especially CH partition 110 (the partition straddling the 55,104,000 split).                                 | Hetzner only.                                                                      |

### Why the split is correctness-driven, not performance-driven

RMT(`<version_column>`) FINAL is correctness-preserving for any column the parser writes as "latest absolute state from XDR" — the row with max version wins, matching sequential-machine output. It is **not** correctness-preserving for:

- `MIN`-semantics columns (any "first_seen" / "earliest" / "minted_at") — RMT keeps the max-version row's value, not the row that recorded the earliest observation.
- Cross-range aggregates (`SUM`, `COUNT`) — per-machine partial aggregates require union-time recomputation.
- Reclassifications that depend on facts observed in both ranges (NFT Phase 3, WASM-based contract typing).

Anything in the second category must run on the union. The pre-merge bucket is strictly things that are either (a) external-dependency lookups (bootstrap RPC) or (b) deterministic local cleanups (0221 drain) — neither benefits from waiting for the union.

## Phased Plan

### Phase 0 — Preconditions (before new workers start)

1. **Land task 0225** on `develop` (sync-validation pre-parse + crash-recovery runbook). Required for laptop 3's invocation, which approaches chain tip and will hit S3 archive lag.
2. **Lock schema version**: tag the current schema migration via the git SHA used for `db-clickhouse-init`. Both workers _and_ Hetzner pin to that SHA. Add a `--require-schema-version` guard to `backfill-runner` so a divergent CH errors out fast.
3. **Apply `users.d/dict.xml` `dict_reader` user fix** in `crates/db-clickhouse/` and the corresponding `init.sql` change to route the `transaction_hash_dict` SOURCE through it. Required because Hetzner uses a non-default password and the current `init.sql` hardcodes `default:clickhouse`. Surfaced trap #2 in task 0216.
4. **Lock parser version**: both machines build `backfill-runner` from the _same git SHA_ on `develop`. Print the SHA at startup and persist it in a new `backfill_runs` audit table (`run_id`, `git_sha`, `schema_version`, `range`, `host`, `started_at`, `finished_at`, `status`).
5. **Worker split boundaries fixed** (all multiples of 64,000, every S3 partition processed by exactly one worker):
   - **Laptop 1 / Machine 2 boundary**: `55,103,999 / 55,104,000` (partition 860/861) — already locked, laptop 1 is in flight
   - **Machine 2 / Laptop 3 boundary**: `60,095,999 / 60,096,000` (partition 938/939)
   - Machine 2 owns 78 partitions (861–938), laptop 3 owns ~38 partitions (939–976) up to `L_last_closed`
     5a. **Upper bound `L_last_closed` is dynamic**: the highest _fully-closed S3 partition_ at laptop 3's completion time, _not_ chain tip. Captured from laptop 3's audit row after its final invocation. Anything beyond `L_last_closed` belongs to a separate live-tail / catch-up effort, not this merge.
     5b. **In-flight density monitoring** (no upfront probe — operator opted to skip and start full backfills directly):
   - **Partition processing order**: oldest-first within each worker's range (default `backfill-runner` behavior). For laptop 3 this means partitions 939 → 976 in ascending order. Density rises gradually within the range → monitoring projection is most informative when the lighter partitions land first and the heavier are still ahead. Newest-first (fail-fast) explicitly rejected — leaves no time for operator response.
   - **Per-partition check** after each S3 partition commits:
     - Read current CH disk: `SELECT sum(bytes_on_disk) FROM system.parts WHERE active`
     - Read partitions completed via `ledgers.sequence` watermark and `audit.backfill_runs`
     - Compute `GB / partition` ratio
     - Projected end-state disk = current used + (remaining partitions × GB/partition)
   - **Trigger threshold**: projected end-state disk > 90 % of worker's filesystem capacity → orchestrator **pauses ingest at the next clean partition boundary**, emits a critical alert (log + audit-table entry with status `paused-disk-pressure`), and waits for operator decision. No automated destructive action — operator explicitly chooses one of:
     - **Continue** (override the alert, accept the risk)
     - **Split-and-drop**: FREEZE + rsync the already-ingested partitions to Hetzner, ATTACH on Hetzner, verify row counts, DROP local CH data, resume with remaining partitions. Loses local debug copy of the first half. Preserves merge correctness.
     - **Abort**: stop, leave local CH intact, plan a different topology.
   - Laptop 3 risk profile is the load-bearing one: 400–500 GB free for the newest 38 partitions. At ≤ 2× density (~370 GB total) → safe. At 2.5× (~430 GB) → marginal, ~50 GB buffer. At ≥ 3× density (~570 GB) → does NOT fit, will trigger the pause threshold around partition ~960–965.
   - Machine 2 risk profile is benign: 761 GB free, fits up to ~1.9× density. Monitor but unlikely to trigger.
6. **Provision additional workers**: ≥ 2 TB disk per worker (runbook §"Target machine prereqs"), follow the full fresh-machine runbook through §5.
7. **Hetzner CH operational readiness** (depends on task 0216 implementation closing out):
   - Ansible playbook from `infra/hetzner/ansible/` has run against `ch-prod-01` (hardening, Docker, ufw, fail2ban, Borg, cron)
   - Caddy + mTLS CA from `infra/hetzner/ca/` deployed, server-side LE cert obtained, `clients-ca.pem` mounted
   - `docker-compose.prod.yml` overlay applied with `ports: !override` (Surfaced trap #1), `profiles: [local-only]` on postgres (Surfaced trap #3), `cap_add: [SYS_NICE, SYS_PTRACE]` on CH (Surfaced trap #4)
   - `crates/db-clickhouse/config.d/memory.xml` tuned for 128 GB / no API contention (Surfaced trap #6): mark cache 16 GiB, per-query 32 GiB, `max_concurrent_queries 50`
   - `db-clickhouse-init` sidecar has run; `SHOW TABLES` confirms 19 tables + 1 dictionary
   - BX21 Storage Box ordered, Borg repo initialized — so post-merge state has a backup target before any prod read traffic. If BX21 is not ready, gate the merge on it.
   - Worker public SSH keys added to `infra/hetzner/ansible/group_vars/all.yml` (or to the deploy user's `authorized_keys`) for the rsync transport — per-worker user accounts preferred so workers can only write to their own staging dir.
   - Per-worker staging directory created on Hetzner host filesystem (same filesystem as `/srv/clickhouse-data`, so `ATTACH PART` hard-links cheaply): `/srv/clickhouse-staging/m1/`, `/srv/clickhouse-staging/m2/`, etc. Owned by the worker user with `0750` perms.
8. **Transport channel**: **FREEZE + rsync + ATTACH PART** per ADR 0045. No S3 intermediary, no `clickhouse-backup`. Per-worker mechanics:

   - On worker, per table: `ALTER TABLE <tbl> FREEZE WITH NAME 'm<id>_<table>'` → creates hard-link snapshots in `/var/lib/clickhouse/shadow/m<id>_<table>/data/default/<tbl>/<part>/`. Hard-links share inodes with active parts → zero extra disk space, instant.
   - Worker rsync: `rsync -avP --partial -e ssh /var/lib/clickhouse/shadow/m<id>_<table>/ deploy@ch-prod-01:/srv/clickhouse-staging/m<id>/<table>/`. Streamed over the worker's uplink. Resumable mid-file via `--partial`. Hetzner side: 1 Gbit unmetered, never the bottleneck.
   - On Hetzner, per worker, per table: look up destination Atomic-engine `<uuid>` via `system.tables.uuid`, `mv` the staged parts into `/var/lib/clickhouse/data/default/<tbl>/detached/`, run `ALTER TABLE <tbl> ATTACH PART '<part_name>'` for each part. Per-part atomic.
   - On worker after Hetzner-side ATTACH success: `ALTER TABLE <tbl> UNFREEZE WITH NAME 'm<id>_<table>'` to release the shadow hardlinks and let local merger compact.

   **Why this beats S3:**

   - Zero local disk overhead during the export window — hardlinks pin inodes, not bytes (vs. ~200–400 GB tarball materialization)
   - No CH downtime on workers (FREEZE is online)
   - Bytes on the wire are already ZSTD-compressed within parts — external compression saves ~10–15%, not worth the dance
   - Parts arrive byte-identical on Hetzner — no re-parse, re-sort, re-compress
   - Resumability at rsync (partial file) + at ATTACH (per part)
   - One hop instead of two; no S3 egress cost; no third-party tooling install on workers

   **What we trade away vs. S3 path:**

   - Worker and Hetzner must both be reachable during the rsync window. `rsync --partial` survives blips; cold-storage durability of S3 is lost. Acceptable for one-time merge.
   - **Atomic-engine `<uuid>` mapping wrinkle** (ADR 0045 cited): source and destination tables have different `<uuid>`s under CH Atomic engine. The mv-to-`detached/` step must look up the destination's UUID per table on Hetzner before each move. Easy to script, easy to fumble — include as a `verify-attach-paths` step in the new orchestrator.

### Phase 1 — Parallel local backfill (3-way split)

- **Laptop 1 (2/5, in flight)**: continues from current watermark to `55,103,999` (end of partition 860) → writes to local CH. No code changes needed; resume logic in `backfill-runner` skips already-committed partitions.

- **Machine 2 (mid-3/5, new)**: provisions local CH with identical schema (`docker compose run --rm db-clickhouse-init` from the locked SHA); single invocation over 78 partitions (861–938):

  ```bash
  ./target/release/backfill-runner --target clickhouse \
      --clickhouse-url "http://127.0.0.1:8123" \
      --soroban-rpc-url "https://mainnet.sorobanrpc.com" \
      run --start 55104000 --end 60095999
  ```

  Range ends exactly at the closed boundary of partition 938 (60,095,999) — a multiple of 64,000. Single clean run, no Phase A/B internal split.

- **Laptop 3 (newest 3/5, new)**: provisions local CH same as machine 2; runs the remaining ~38 partitions (939 → highest fully-closed S3 partition at run-start), processing partitions oldest-first per Phase 0 step 5b:

  ```bash
  # Pick --end as the highest closed-partition boundary minus 1 at run-start.
  # Task 0225's sync-validation pre-parse additionally guards any partition
  # that turns out incomplete when the run reaches it.
  ./target/release/backfill-runner --target clickhouse \
      --clickhouse-url "http://127.0.0.1:8123" \
      --soroban-rpc-url "https://mainnet.sorobanrpc.com" \
      run --start 60096000 --end <L_last_closed_at_run_start>
  ```

  After completion, record the actual `L_last_closed` reached (from the audit table) — that becomes the canonical merge upper bound. If more S3 partitions have closed by then, an optional follow-up invocation extends `L_last_closed` further before export. Each invocation is independent and idempotent (runbook §11).

  **Disk-pressure pause** (Phase 0 step 5b): after each S3 partition commit, orchestrator computes projected end-state disk; if > 90 % of filesystem capacity, pause at next clean boundary and prompt operator (continue / split-and-drop / abort).

- All workers record progress in `backfill_runs` (new audit table — see Critical Files).
- All workers monitor via the runbook §7 watch queries; eject early on schema-version / git-SHA drift.

### Phase 2 — Pre-merge per-machine cleanup + invariants

Before export, run **on each local CH**:

1. **Final bootstrap top-up** (runbook §8a) over the machine's own range:
   ```bash
   backfill-runner ... bootstrap --start <range_start> --end <range_end>
   ```
   Expect `discovered=N fetched=M staged=M rpc_errors=0`. Idempotent.
2. **Drain 0221 SAC leak** (runbook §8b) — `ALTER TABLE nfts_pending DELETE WHERE …` then `OPTIMIZE TABLE nfts_pending FINAL`. Cheaper to drain locally than ship junk rows to Hetzner.
3. **Baseline metrics** (runbook §8c query) — capture per-machine `accounts / skeletons / classic_credits / sac_contracts / nfts_hot / nfts_pending` counts into a `pre-export-metrics.json` artifact. Used for Phase 6 row-count parity.
4. **Run new `backfill-runner verify-local` subcommand**:
   - **No gaps within range**: `SELECT MIN(sequence), MAX(sequence), count() FROM ledgers` matches `[range_start, range_end]` continuous.
   - **Tx-per-ledger count vs XDR-expected**: cross-check `count() GROUP BY ledger_sequence` against a deterministic recount of source XDR.
   - **No orphan ops**: `SELECT count() FROM operations_appearances WHERE transaction_id NOT IN (SELECT id FROM transactions)` = 0.
   - **Schema + parser-sha parity**: log schema migration version and parser git SHA; merge orchestrator refuses to proceed if they differ.
   - **Disjointness check** across the two machines via shared `backfill-runs.json`.

### Phase 3 — Export from local CHs (FREEZE + rsync per ADR 0045)

Per worker, executed by a new `scripts/freeze-and-rsync-to-hetzner.sh` orchestrator:

1. **FREEZE all RMT + MergeTree tables** in `default`:

   ```sql
   ALTER TABLE ledgers                       FREEZE WITH NAME 'm<id>_ledgers';
   ALTER TABLE wasm_interface_metadata       FREEZE WITH NAME 'm<id>_wim';
   ALTER TABLE accounts                      FREEZE WITH NAME 'm<id>_accounts';
   ALTER TABLE soroban_contracts             FREEZE WITH NAME 'm<id>_sc';
   ALTER TABLE assets                        FREEZE WITH NAME 'm<id>_assets';
   ALTER TABLE account_balances_current      FREEZE WITH NAME 'm<id>_abc';
   ALTER TABLE nfts                          FREEZE WITH NAME 'm<id>_nfts';
   ALTER TABLE nfts_pending                  FREEZE WITH NAME 'm<id>_nfts_pending';
   ALTER TABLE liquidity_pools               FREEZE WITH NAME 'm<id>_lp';
   ALTER TABLE lp_positions                  FREEZE WITH NAME 'm<id>_lpp';
   ALTER TABLE transactions                  FREEZE WITH NAME 'm<id>_tx';
   ALTER TABLE transaction_hash_index        FREEZE WITH NAME 'm<id>_thi';
   ALTER TABLE operations_appearances        FREEZE WITH NAME 'm<id>_oa';
   ALTER TABLE transaction_participants      FREEZE WITH NAME 'm<id>_tp';
   ALTER TABLE soroban_events                FREEZE WITH NAME 'm<id>_se';
   ALTER TABLE soroban_invocations_appearances FREEZE WITH NAME 'm<id>_sia';
   ALTER TABLE nft_ownership                 FREEZE WITH NAME 'm<id>_no';
   ALTER TABLE nft_ownership_pending         FREEZE WITH NAME 'm<id>_nop';
   ALTER TABLE liquidity_pool_snapshots      FREEZE WITH NAME 'm<id>_lps';
   ```

   Each is online, instantaneous, refcount-only on the hard-link layer. Zero extra disk used.

2. **rsync the shadow tree to Hetzner**, per table, into per-worker staging dir:

   ```bash
   rsync -avP --partial --bwlimit=<optional> \
       /var/lib/clickhouse/shadow/m<id>_<table>/data/default/<table>/ \
       deploy@ch-prod-01:/srv/clickhouse-staging/m<id>/<table>/
   ```

   Log per-table success to a worker-local `freeze_runs` audit table.

3. **Verify rsync completion** before signaling Hetzner: count parts shipped vs. parts FROZEN, fingerprint-check (size + mtime) for each part directory.

4. After Hetzner-side ATTACH succeeds (Phase 4), per worker:
   ```sql
   ALTER TABLE <tbl> UNFREEZE WITH NAME 'm<id>_<table>';
   ```
   Releases hardlinks; worker local CH merger free to compact superseded parts.

**Export ordering doesn't matter at this stage** — order matters at ATTACH time (Phase 4) for RMT version-column semantics, not at upload time.

### Phase 4 — Import to Hetzner (ATTACH PART, ordered)

**Order matters** because `ReplacingMergeTree(<version>)` keeps the row with max version. Attaching laptop 1's oldest-range parts first, then machine 2's, then laptop 3's newest-range parts on top → background merger eventually picks the newer-range row → matches single-machine sequential outcome.

Per-table loop in `scripts/attach-parts-on-hetzner.sh` running on Hetzner:

1. Look up destination Atomic-engine UUID:
   ```sql
   SELECT uuid FROM system.tables WHERE database = 'default' AND name = '<tbl>'
   ```
2. For each worker in ascending range order (m1 → m2 → m3 → m4), for each part directory in `/srv/clickhouse-staging/m<id>/<tbl>/`:
   ```bash
   sudo mv /srv/clickhouse-staging/m<id>/<tbl>/<part_name>/ \
       /var/lib/clickhouse/store/<uuid_prefix>/<uuid>/detached/<part_name>/
   sudo chown -R 101:101 /var/lib/clickhouse/store/<uuid_prefix>/<uuid>/detached/<part_name>/
   ```
   (uuid_prefix = first 3 hex chars of uuid per CH Atomic-engine layout.)
3. ATTACH:
   ```sql
   ALTER TABLE <tbl> ATTACH PART '<part_name>';
   ```
   Per-part atomic. If a part is malformed (rare; parts are byte-identical between hosts), the ATTACH fails and that one part can be re-rsynced.
4. After all parts of a partitioned table are attached: `OPTIMIZE TABLE <tbl> FINAL PARTITION <p>` per CH partition. Pay special attention to **CH partition 110** (the straddle, `intDiv(55,104,000, 500,000)`).
5. Log per-`(worker, table, part)` success in a `attach_imports` audit table on Hetzner.
6. After all tables landed: `chown -R clickhouse:clickhouse /var/lib/clickhouse/store/` defensive sweep (in case any mv missed the chown).

The orchestrator is resumable on the audit table — re-running picks up where a failure left off.

### Phase 5 — Post-merge repair on Hetzner

Run in this order (each step idempotent):

Run in this order (each step idempotent):

1. `OPTIMIZE TABLE <tbl> FINAL` for every RMT table (parallelize per partition where memory allows; CH partition 110 is the critical one).
2. **Dedup the two plain `MergeTree` tables** (Tier 2 of the danger list):
   ```sql
   OPTIMIZE TABLE wasm_interface_metadata FINAL DEDUPLICATE BY wasm_hash;
   OPTIMIZE TABLE ledgers FINAL DEDUPLICATE BY sequence;
   ```
3. **Bootstrap subcommand over the full union range** (runbook §8a, against Hetzner) — fills any skeleton accounts (`sequence_number = 0`) still present after merge. Auto-watermark fix in `bootstrap.rs` guarantees the snapshot stamp wins all RMT races (commit `dec2a49`).
4. **Drain 0221 SAC leak on Hetzner** (runbook §8b) — even if drained per-machine, a final sweep is cheap insurance.
5. **Rebuild Tier-1 columns** via staging-table + `EXCHANGE TABLES` (cheaper and more observable than `ALTER … UPDATE` mutations at multi-TB scale). One staging swap per column-group:
   - `accounts.first_seen_ledger = MIN(ledger_sequence)` from `transaction_participants` per `account_id`.
   - `lp_positions.first_deposit_ledger = MIN(ledger_sequence)` from `operations_appearances` filtered to deposit ops, grouped by `(pool_id, account_id)`.
   - `nfts.{minted_at_ledger, collection_name, name, media_url}`: `minted_at_ledger = MIN(ledger_sequence) WHERE event_type = Mint` from `nft_ownership`; metadata fields rehydrated from the mint event's payload (XDR re-parse if not persisted on `nft_ownership`).
   - `nfts_pending` same as `nfts`.
   - `soroban_contracts.{deployer_id, deployed_at_ledger}` from `operations_appearances` filtered to `Create*Contract*` ops per `contract_id`.
6. **NFT Phase 3 reclassification** (task 0118 cleanup SQL) — reclassify `Other` contracts where WASM is now visible across the union. Affects `soroban_contracts.contract_type`.
7. **`nfts_pending` promotion** — re-run promotion logic after step 5–6.
8. **Asset aggregates** (task 0194) — `total_supply`, `holder_count` from `account_balances_current`.
9. **Statement B partial-index rebuild** (task 0198) — gate on that PR landing.

### Phase 6 — End-to-end validation

New `backfill-runner verify-completeness` subcommand, run against Hetzner:

1. **Ledger continuity**: no gaps in `ledgers.sequence` from `50,457,424` to `L_last_closed` (the canonical merge upper bound captured from laptop 3's audit row). Anything past `L_last_closed` is out of scope and the API surface should advertise completeness only up to that ledger.
2. **Row-count parity**: per `(table, CH partition)`, `count_local_laptop1 + count_local_machine2 + count_local_laptop3` versus `count() FROM tbl FINAL` on Hetzner (allowing RMT collapse). Use the per-worker `pre-export-metrics.json` from Phase 2 as the input expectation.
3. **Per-ledger tx count** matches XDR-expected (re-derive from S3).
4. **No orphan ops** (as in Phase 2, but at Hetzner scope).
5. **Account balance non-null** for every account that has any operation.
6. **Sequence-number monotonicity** per account: `lagInFrame` check.
7. **Skeleton percentage** (runbook §8c) < 1% on Hetzner after Phase 5 bootstrap.
8. **Sample compare against Horizon / stellar.expert** for a randomized stratified sample of 1000 ledgers across the union range. Reuse existing `compare-with-stellar-api` skill.
9. **Diff against truth via existing `scripts/diff-merge-vs-truth.sh`** if applicable.

---

## Critical Files / New Code

**Existing — to reuse**:

- `crates/backfill-runner/src/{run,sync,error,bootstrap,sink}.rs` — runner core; `bootstrap` already gives skeleton-account enrichment (runbook §8a).
- `crates/backfill-runner/src/main.rs:54-59` — `--clickhouse-url` / `CLICKHOUSE_URL` plumbing.
- `scripts/diff-merge-vs-truth.sh`, `scripts/gen-merge-snapshots.sh`, `scripts/run-merge-snapshots.sh` — snapshot/diff infra already present, reuse for per-partition validation.
- `docs/runbooks/backfill_soroban_2of5_fresh_machine.md` — parent runbook for fresh-machine setup; extend rather than fork.
- `docs/runbooks/0225_backfill_crash_recovery.md` (in flight) — referenced from §9 of the 2/5 runbook; resume semantics for archive-lag failures.
- `docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md` — drain procedure referenced in §8b.
- **ADR 0045** (`lore/2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md`) — the FREEZE + rsync + ATTACH PART transport design. This plan is the N-worker variant of the same mechanic.
- **ADR 0040** (`lore/2-adrs/0040_multi-laptop-backfill-snapshot-merge-hazards.md`) — table-by-table merge semantics from the PG era; CH version is simpler (deterministic CityHash IDs eliminate ADR 0040's biggest cost, FK remap) but the watermark/dangling-ref / partition / seed-data hazards still apply.
- **Task 0216** (`lore/1-tasks/active/0216_RESEARCH_hetzner-clickhouse-deploy/`) — Hetzner deployment reference, surfaced traps, mTLS CA layout, BX21 backup plan. **Surfaced traps directly relevant: #1 `ports: !override`, #2 `dict_reader` user, #3 `profiles: [local-only]` on postgres, #4 `cap_add` on CH, #6 memory tuning for 128 GB no-API box.**
- `infra/hetzner/ansible/` (to be authored under task 0216 implementation work) — provisioning playbook for `ch-prod-01`; the merge plan assumes the playbook has run.
- `infra/hetzner/ca/` — mTLS CA scripts; client cert distribution for dev/AWS access (not for the rsync transport, which uses SSH).
- Task 0118 Phase 3 reclassification SQL.
- Task 0214 `bootstrap` subcommand (commit `dec2a49`).

**New (to be added in implementation)**:

- `crates/backfill-runner/src/audit.rs` + migration: `backfill_runs` / `freeze_runs` / `attach_imports` tables tracking `(run_id, git_sha, schema_version, host, range, started_at, finished_at, status)` per phase.
- `crates/backfill-runner/src/verify_local.rs` — new `verify-local` subcommand for Phase 2 invariants + skeleton-percentage gate.
- `crates/backfill-runner/src/verify_completeness.rs` — new `verify-completeness` subcommand for Phase 6.
- `scripts/freeze-and-rsync-to-hetzner.sh` — Phase 3 orchestrator (FREEZE + rsync + audit).
- `scripts/attach-parts-on-hetzner.sh` — Phase 4 orchestrator (UUID lookup, mv-to-`detached`, `ATTACH PART`, post-attach `OPTIMIZE FINAL PARTITION`, audit).
- `scripts/repair-post-merge.sql` — Phase 5 SQL bundle (`OPTIMIZE FINAL`, `DEDUPLICATE BY`, Tier-1 column rebuilds via staging + `EXCHANGE TABLES`, NFT Phase 3 trigger).
- `docs/runbooks/merge-parallel-backfills.md` — operator runbook (extends the 2/5 fresh-machine runbook + cites ADR 0045 + task 0216).
- Spawn a **new lore task** for this work (this synthesis lives under `0228_FEATURE_parallel-backfill-merge-and-validation`) — separate from 0225's scope of sync-validation pre-parse + crash-recovery runbook. Note: ID resolved to 0228 (not 0227) after a parallel commit on `develop` claimed 0227 for an infra-hetzner-ansible task.
- If schema-engine swap is taken up later, separate task (`0229_PROPOSAL_aggregatingmergetree-for-state-tables`).

---

## Verification (end-to-end)

After Phase 6 passes, run these as a final gate:

```bash
# 1. Ledger continuity + row-count parity
backfill-runner verify-completeness --clickhouse-url $HETZNER_URL \
    --expected-runs <run_id_laptop1>,<run_id_machine2>,<run_id_laptop3> \
    --sample-ledgers 1000

# 2. Cross-source spot check (manual or via skill)
#    use /compare-with-stellar-api skill on the sampled ledgers

# 3. Schema/parser audit
clickhouse-client --host $HETZNER --query "
    SELECT run_id, git_sha, schema_version, host, range_start, range_end, status
    FROM backfill_runs ORDER BY started_at"
```

Acceptance criteria for "merge complete":

- All Phase 6 checks green.
- `compare-with-stellar-api` sample shows ≤ 0.01% mismatch attributable to Horizon/Stellar.expert lag, none attributable to our pipeline.
- Repair pass mutations all reported `done` in `system.mutations`.

---

## Open Questions / Risks (worth raising before implementation)

1. **Disk pressure on workers** with the 3-way split:
   - **Laptop 1 (2/5)**: measured 180 GB at 60 % → ~300 GB end-state on its 1 TB SSD, with 400 GB free at end of 2/5. Fits FREEZE + rsync window comfortably.
   - **Machine 2 (78 partitions, mid-3/5)**: **761 GB free**. Linear-density projection ~322 GB; at 1.5× density ~483 GB; at 2× density ~644 GB (peak ~752 GB with FREEZE + scratch + ext4 reserved → effectively zero buffer). Single-shot run is **safe up to ~1.9× density**; needs split-into-halves at higher densities.
   - **Laptop 3 (newest 3/5, ~38 partitions)**: 400–500 GB free. Density highest here (Aug 2025 – May 2026 Soroban activity ramp); 2–3× of laptop 1's measured density plausible. At 2× density needs ~370 GB total (fits). At 2.5× ~430 GB (marginal). At ≥ 3× ~570 GB (does NOT fit, requires mid-run split).
   - Operator chose no upfront probe → in-flight monitoring (Phase 0 step 5b) is the safety net. The fallback "stop, FREEZE + rsync what's done, drop local, resume" path must be runbook-ready BEFORE laptop 3 starts.
2. **`ALTER TABLE … UPDATE` mutation cost** on the repair pass — for large `accounts` / `lp_positions` tables, mutations are heavy. Plan uses staging copy + `EXCHANGE TABLES` instead. Worth benchmarking on a pilot once table sizes are concrete.
3. **Task scope**: this is broader than 0225. Lands under `0228_FEATURE_parallel-backfill-merge-and-validation` (0227 was claimed by an infra-hetzner-ansible task via a parallel commit) with 0225 + 0216 as prerequisites.
4. **Hetzner CH topology — single-node confirmed by task 0216** (Server Auction box, single CH 26.3 in Docker). No `ReplicatedMergeTree` complication. HA is explicit out-of-scope for now; if/when it lands, a separate ADR will trigger a schema rewrite and this plan will need revisit.
5. **Repair pass blocks on tasks 0118/0194/0198/0199** — at least 0118 (NFT reclass) and 0198 (Statement B). Surface in the new task's `blocked_by`.
6. **Task 0225 dependency**: laptop 3's invocation (`--start 60096000 --end <L_last_closed>`) approaches chain tip and will hit S3 archive lag; the sync-validation pre-parse on the current branch is the right fix. Either land 0225 first, or run laptop 3 from `feat/0225_*` directly with `--require-schema-version` enforcement. Machine 2's range (ends at 60,095,999) is far enough from tip to not need 0225 — but pinning to the same SHA is required for parser parity.
7. **`pre-export-metrics.json` shared via where?** Recommend the `backfill_runs` audit table — single source of truth, indexable.
8. **ADR 0044 staleness** — task 0216's "Future Work" section already plans a net-new ADR superseding 0044's "pilot" framing to record the architectural realignment (Postgres-on-RDS abandoned, CH-on-Hetzner sole prod store, mTLS over Tailscale). This plan can reference that pending ADR without owning it. Coordinate with whoever lands the 0216 implementation work.
9. **NFT metadata rehydration**: confirm whether the mint event payload (`collection_name`, `name`, `media_url`) is decodable directly from `soroban_events.{topics_xdr, data_xdr}` on Hetzner. If yes, the Tier-1 repair stays cheap (a few minutes via in-CH ScVal decode using the NFT-contract filter). If no, we'd need to re-parse mint operations from S3 — adds 1–3 h to the repair budget. Audit before committing.
10. **Soroban RPC routing for parallel workers.** In sequential single-machine backfill, the per-window bootstrap RPC pass averages 1–5 RPS — well below public RPC's ~50 RPS Cloudflare cap. In N-way parallel backfill, if all workers point at the same public RPC, combined load becomes N × per-worker rate and routinely trips the 1015 throttle. Three viable answers:
    - Route each worker to a different public RPC endpoint
    - One private RPC tier shared across workers sized for N × 50 RPS (~$50–200/month for the backfill duration) — recommended
    - Self-host Soroban RPC on Hetzner — biggest infra burden (~2–7 days sync, ~$200–500/month), reusable for realtime ingest in production
      Also add `verify-local`: gate export on `countIf(sequence_number = 0) / count() < 1%` per machine.
11. **BX21 ordering** — task 0216 lists BX21 Storage Box as "not yet ordered" (2026-05-15). Borg → BX21 is the canonical production backup destination. Order BX21 + initialize Borg repo as a Phase 0 prerequisite so the post-merge state is backed up before any read traffic lands. If BX21 is unavailable for any reason, **block the merge** rather than ship without a backup target.
12. **`assignPublicIp: ENABLED` on Galexie ECS** — task 0216 calls this out as load-bearing for the AWS-side ingest path. Not directly affected by the merge plan, but a CDK review for any change to `IngestionStack` should flag it. Cite the trap here so the merge-implementation PR review surfaces it.
13. **No `clickhouse-backup` install needed on workers.** Earlier draft had this; the FREEZE + rsync + ATTACH PART approach uses standard CH SQL + rsync + SSH only. Drop from the worker provisioning runbook.
14. **mTLS client cert for verification queries** — if `verify-completeness` or any direct Hetzner query is needed from outside the box (e.g. running validation from a developer laptop), the laptop needs an mTLS client cert from `infra/hetzner/ca/`. Issue per-developer certs as part of Phase 0 if validation runs externally. Alternative: run `verify-completeness` from the Hetzner host itself via `docker exec`, no cert needed.
