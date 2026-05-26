---
title: 'CH Snapshot B — operator runbook'
type: generation
status: seed
spawned_from: ../README.md
spawns: []
tags: [ops, hetzner, backup, runbook]
links:
  - ../README.md
history:
  - date: '2026-05-26'
    status: seed
    who: stkrolikiewicz
    note: >
      Skeleton created during Phase 0. Operator fills in
      command shapes from shell history (Snapshot A creation)
      and verifies against running CH instance.
---

# CH Snapshot B — operator runbook

Reconstructed procedure for taking the post-0252 CH snapshot
on `sorban-prod`. Anchored to the gaps surfaced during 0260
Phase 0 exploration: no committed BACKUP runbook in the repo,
no committed M2↔Hetzner transport doc.

## Topology clarification

The task uses "M2" as shorthand for the **fishuser-HERO** host
(a Linux worker that ran one leg of the 0228 parallel backfill).
It is the rsync destination and the host that needs its old
backfill removed.

## Transport

Direct SSH on alias `sorban-prod` from fishuser-HERO. No
wireguard. SSH key for `sorban-prod` must be loaded on
fishuser-HERO's agent before the rsync step.

## 0. Prerequisites

- SSH agent on fishuser-HERO has `sorban-prod` key loaded
  (`ssh-add -l`).
- 0252 closed (artifact: `docs/runbooks/artifacts/endpoint_validation_20260525.md`).
- fishuser-HERO has free disk ≥ estimated Snapshot B size + headroom
  **after backfill Docker volume removed**. Measured 2026-05-26:
  393 GiB free pre-delete → 760 GiB free post-delete → ~68 GiB
  (9 %) headroom for a 692 GiB snapshot. Below the 15 % comfort
  threshold but workable; if Snapshot B comes in larger than
  estimate, abort and free more space.
- Hetzner has free disk ≥ estimated Snapshot B size + 15 %
  headroom **after Snapshot A delete**. Measured 2026-05-26:
  280 GiB free pre-delete → 972 GiB free post-delete → 28 %
  headroom for 692 GiB snapshot. Comfortable.

## 1. Pre-flight disk audit

```bash
ssh sorban-prod 'df -h /srv'
ssh sorban-prod 'du -sh /srv/backups/* /srv/clickhouse-data 2>/dev/null'
```

**Captured values (2026-05-26):**

- Root FS (`/dev/md1`, mounted at `/`): 1.8 TiB total, 1.4 TiB used, **280 GiB free**.
  `/srv` lives on the same FS.
- Snapshot A (`/srv/backups/pre_repair_20260521_1502`): **692 GiB**.
  ✓ Path confirmed via `ls -la /srv/backups/` — **no trailing `]`**.
  Memory + task README had a typo (transcribed `…1502]/`). Real
  dirname is plain `pre_repair_20260521_1502`.
- `/srv/clickhouse-data` raw size: **693 GiB**.
- Snapshot B estimate: ~**692 GiB** (Snapshot A:CH-data ratio ≈ 1.0 —
  zstd at default level on already-compressed MergeTree data
  doesn't shrink much; expect Snapshot B ~ raw CH size).
- Post-A-delete free disk: 280 + 692 = **972 GiB**.
- Snapshot B headroom: 972 − 692 = **280 GiB (28 %)** — above 15 % threshold.
- **Go/no-go: GO.**

## 2. Enumerate CH tables

```bash
docker exec -i app-clickhouse-1 clickhouse-client --multiquery <<'EOF'
SELECT name FROM system.tables
WHERE database = 'default'
ORDER BY name FORMAT TSV;
EOF
```

**Confirmed live enumeration on `app-clickhouse-1` (2026-05-26):**

20 entities in `default`:

| Name | Engine |
|------|--------|
| `account_balances_current` | ReplacingMergeTree |
| `accounts` | ReplacingMergeTree |
| `assets` | ReplacingMergeTree |
| `ledgers` | **MergeTree** |
| `liquidity_pool_snapshots` | ReplacingMergeTree |
| `liquidity_pools` | ReplacingMergeTree |
| `lp_positions` | ReplacingMergeTree |
| `nft_ownership` | ReplacingMergeTree |
| `nft_ownership_pending` | ReplacingMergeTree |
| `nfts` | ReplacingMergeTree |
| `nfts_pending` | ReplacingMergeTree |
| `operations_appearances` | ReplacingMergeTree |
| `soroban_contracts` | ReplacingMergeTree |
| `soroban_events` | ReplacingMergeTree |
| `soroban_invocations_appearances` | ReplacingMergeTree |
| `transaction_hash_dict` | **Dictionary** |
| `transaction_hash_index` | ReplacingMergeTree |
| `transaction_participants` | ReplacingMergeTree |
| `transactions` | ReplacingMergeTree |
| `wasm_interface_metadata` | MergeTree |

Matches `crates/db-clickhouse/schema/init.sql:87-408` exactly.
`BACKUP DATABASE default` captures all 20 entities atomically
(including the `transaction_hash_dict` dictionary metadata).

## 3. Memory cap — defer bump unless BACKUP fails

Current `max_memory_usage = 6,000,000,000` (6 GiB) — measured
2026-05-26 via `system.settings`. Snapshot A presumably succeeded
on the same cap (no record of a bump in box bash history grep);
attempt Snapshot B with the default first.

**If BACKUP fails with `MEMORY_LIMIT_EXCEEDED`:**

```bash
sudo $EDITOR /srv/app/crates/db-clickhouse/users.d/timeouts.xml
# Raise cap (e.g. 32_000_000_000 = 32 GiB)
docker restart app-clickhouse-1
docker exec -i app-clickhouse-1 clickhouse-client --multiquery <<'EOF'
SELECT value FROM system.settings WHERE name = 'max_memory_usage';
EOF
```

Revert after the BACKUP completes (Section 5).

## 4. Run BACKUP

**Confirmed shape from Snapshot A creation** (recovered from box bash
history, lines 234-242):

```sql
BACKUP DATABASE default
TO Disk('backups', 'snapshot_b_post_0252_<YYYYMMDD>');
```

Recommended wrapper for Snapshot B:

```bash
BACKUP_NAME="snapshot_b_post_0252_$(date -u +%Y%m%d)"
docker exec -i app-clickhouse-1 clickhouse-client --multiquery <<EOF
BACKUP DATABASE default TO Disk('backups', '${BACKUP_NAME}');
EOF
```

Key points (vs the earlier placeholder this section had):

- `BACKUP DATABASE default` is **one statement**, not a per-table
  loop. Covers all tables + dictionaries in the `default` database
  atomically. No table enumeration needed inside the BACKUP query.
- No `SETTINGS compression_method='zstd'` was used for Snapshot A
  — zstd is the default on `Disk('backups', …)` in this CH build,
  per the on-disk size of A (692 GiB) closely matching raw
  `/srv/clickhouse-data` (693 GiB), which already stores parts
  with the table-level compression codec.
- Disk name `backups` is registered in CH's `storage_configuration`
  and maps to host `/srv/backups/` (via the container's
  `/backups` mount). Confirmed by `ls /srv/backups/` matching
  the snapshot dirname.

### Pre-conditions before BACKUP

- **No OPTIMIZE running.** Snapshot A creation explicitly waited
  for OPTIMIZE to finish ("don't backup pre-OPTIMIZE state",
  history line 234). Check before Snapshot B:
  ```bash
  docker exec -i app-clickhouse-1 clickhouse-client --multiquery <<'EOF'
  SELECT query_id, query, elapsed
  FROM system.processes
  WHERE query ILIKE '%OPTIMIZE%'
  FORMAT Vertical;
  EOF
  ```
- **No prior partial snapshot residue.** History line 354 shows a
  past `BACKUP DATABASE default TO Disk('backups', 'snapshot_b_post_phase5')`
  attempt that produced no on-disk artifact (only `pre_repair_*`
  is listed by `ls /srv/backups/`). Check `system.backups` for
  any FAILED rows that might block a re-run:
  ```bash
  docker exec -i app-clickhouse-1 clickhouse-client --multiquery <<'EOF'
  SELECT id, name, status, start_time, end_time, error
  FROM system.backups
  ORDER BY start_time DESC
  LIMIT 10
  FORMAT Vertical;
  EOF
  ```

Capture during the run:
- Start time: __
- End time: __
- Total wall: __
- Final compressed size on disk: __
- CH-reported `bytes` / `total_size` from `system.backups`: __

## 5. Revert memory cap (only if Section 3 bumped it)

Skip if Section 3 was not triggered.

```bash
sudo $EDITOR /srv/app/crates/db-clickhouse/users.d/timeouts.xml  # restore 6 GiB
docker restart app-clickhouse-1
docker exec -i app-clickhouse-1 clickhouse-client --multiquery <<'EOF'
SELECT value FROM system.settings WHERE name = 'max_memory_usage';
EOF
```

## 6. Row-count integrity check

Compare against the frozen baseline in `[[ch-backfill-state]]` memory.

```bash
docker exec -i app-clickhouse-1 clickhouse-client --multiquery <<'EOF'
SELECT 'ledgers', count() FROM ledgers
UNION ALL SELECT 'transactions', count() FROM transactions
UNION ALL SELECT 'transaction_participants', count() FROM transaction_participants
UNION ALL SELECT 'operations_appearances', count() FROM operations_appearances
UNION ALL SELECT 'soroban_events', count() FROM soroban_events
UNION ALL SELECT 'soroban_invocations_appearances', count() FROM soroban_invocations_appearances
UNION ALL SELECT 'soroban_contracts', count() FROM soroban_contracts
UNION ALL SELECT 'transaction_hash_index', count() FROM transaction_hash_index
UNION ALL SELECT 'account_balances_current', count() FROM account_balances_current
UNION ALL SELECT 'accounts', count() FROM accounts
UNION ALL SELECT 'liquidity_pool_snapshots', count() FROM liquidity_pool_snapshots
UNION ALL SELECT 'liquidity_pools', count() FROM liquidity_pools
UNION ALL SELECT 'lp_positions', count() FROM lp_positions
UNION ALL SELECT 'assets', count() FROM assets
UNION ALL SELECT 'nfts_pending', count() FROM nfts_pending
UNION ALL SELECT 'nft_ownership_pending', count() FROM nft_ownership_pending
UNION ALL SELECT 'wasm_interface_metadata', count() FROM wasm_interface_metadata
FORMAT TSV;
EOF
```

Drift threshold: zero. CH BACKUP is consistent by design; any
delta means concurrent writes (write path should be off pre-0241).

## 6.5 Free fishuser-HERO disk (pre-rsync)

Snapshot B is too large to fit alongside the orphaned backfill
volume on fishuser-HERO (last query 2026-05-21; idle 5 days;
frozen at 0228 merge state, no live ingest).

```bash
# On fishuser-HERO:
cd ~/Desktop/soroban/soroban-block-explorer  # or wherever the
                                             # compose file lives
docker compose ps clickhouse                  # confirm it's the
                                             # right service
docker compose down clickhouse                # stop the container
docker volume rm soroban-block-explorer_clickhouse-data
df -h /                                       # confirm ~760 GiB free
```

Capture freed bytes for the README history entry.

**Cautions:**
- Do `docker compose down` for just the clickhouse service if other
  services in the stack are needed running. Otherwise full `down`
  is fine.
- If `docker volume rm` errors "volume in use", a container is
  still attached — repeat `compose down` against the right
  service, or `docker rm -f <id>` the lingering container.

## 7. rsync to fishuser-HERO

Operator drives from fishuser-HERO (pull):

```bash
# On fishuser-HERO, sorban-prod key loaded in agent:
mkdir -p ~/snapshots
rsync -avzP --partial --human-readable \
  sorban-prod:/srv/backups/snapshot_b_post_0252_<YYYYMMDD>/ \
  ~/snapshots/snapshot_b_post_0252_<YYYYMMDD>/
```

Notes on rsync flags:
- `-z` adds zstd-over-SSH wire compression; the snapshot is already
  zstd-compressed on disk so `-z` gains little and burns CPU. Drop
  if the bottleneck is CPU rather than bandwidth.
- `-P --partial` lets a flaky transfer resume from where it stopped.
- Path `~/snapshots/` is a suggested destination on fishuser-HERO's
  916 GiB root; adjust if there's a more conventional snapshot dir.

Capture:
- Start time: __
- End time: __
- Avg MB/s: __
- Total wall: __
- Total bytes: __

## 8. md5 verification

```bash
# Hetzner:
ssh sorban-prod 'cd /srv/backups/snapshot_b_post_0252_<YYYYMMDD> && \
  find . -type f -exec md5sum {} \; | sort' > /tmp/snapshot_b_remote.md5
# fishuser-HERO (run locally on the host where rsync landed):
cd ~/snapshots/snapshot_b_post_0252_<YYYYMMDD>
find . -type f -exec md5sum {} \; | sort > /tmp/snapshot_b_local.md5
diff /tmp/snapshot_b_remote.md5 /tmp/snapshot_b_local.md5
# Zero-diff = green.
```

## 9. Snapshot A removal — single-step

⚠ Earlier draft of this runbook proposed `mv` to `.trash_*` for
a recovery window. That is **wrong on a single-filesystem box**:
`mv` only rewrites the directory entry, the 692 GiB of blocks
stay allocated, so post-`mv` `df` shows the same free space.
There is no other large filesystem on `sorban-prod` to receive
the rename.

Single-step delete is the correct path. The risk surface is
narrower than it looks: the live `/srv/clickhouse-data` is the
source of truth, the indexer write path is off pre-0241, and
a failed Snapshot B is recoverable by re-running BACKUP against
the same live state. A's job as a Phase 5 rollback gate ended
when 0252 closed.

```bash
ssh sorban-prod 'rm -rf /srv/backups/pre_repair_20260521_1502'
ssh sorban-prod 'df -h /srv'   # confirm ~972 GiB free
```

Capture freed bytes for the README history entry.

## Open questions for operator

1. ~~Was Snapshot A taken with a single multi-table BACKUP or a
   per-table loop?~~ **Resolved**: single `BACKUP DATABASE default`
   statement (box bash history lines 235-238).
2. ~~What `max_memory_usage` value did Snapshot A use?~~ **Resolved**:
   current live value is 6 GiB (no bump in place). No record of a
   bump in box bash history during Snapshot A creation. Plan: try
   Snapshot B at 6 GiB first; bump only on `MEMORY_LIMIT_EXCEEDED`.
3. ~~Where on M2 does the old Soroban-era backfill live? Size?~~
   **Resolved**: "M2" = fishuser-HERO. Backfill was the live
   Docker volume `soroban-block-explorer_clickhouse-data`
   (367 GiB) at `/var/lib/docker/volumes/…/_data`. Removed
   2026-05-26 (Phase 1), 760 GiB free now.
4. ~~Wireguard or SSH?~~ **Resolved**: direct SSH on alias
   `sorban-prod` from fishuser-HERO. No wireguard configured.
5. ~~Does the `transaction_hash_dict` dictionary need explicit
   BACKUP coverage?~~ **Resolved**: `BACKUP DATABASE default`
   covers all tables + dictionaries in the database atomically;
   no separate handling.
6. ~~Does `system.backups` show a FAILED row from the prior
   `snapshot_b_post_phase5` attempt?~~ **Resolved**: `system.backups`
   is empty (2026-05-26). No residual rows, clean slate.
7. **New:** no OPTIMIZE was running at audit time (self-match
   only); operator must re-check immediately before issuing
   the BACKUP command, since OPTIMIZE can start at any time
   via merge schedule.

Answers feed back into this note + the [[hetzner-ch-artifacts]]
memory entry.
