# Runbook: derived-table re-parse backfill, directly on the Hetzner box

**Goal:** populate a **new** derived ClickHouse table over Soroban-era history,
running **directly on the prod box** (`sorban-prod`) — no local-then-mirror
(FREEZE/rsync/ATTACH, [ADR 0045](../../lore/2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md)).
Used for `operation_asset_appearances` (tasks [0359]/[0379]) and
`operation_pools` (0365), 2026-07-13→16 — see the [worked example](#appendix--worked-example-operation_asset_appearances-035903790365-2026-07-13-16).

**Idempotent:** yes. Every target is `ReplacingMergeTree`; re-parsing an
already-ingested range collapses on merge. Safe to interrupt and resume.

---

## 0. Which flavour do you have?

A "new derived table over history" is one of two very different jobs:

| Flavour                             | When                                                                                                          | Cost                                       | Method                                              |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------- | ------------------------------------------ | --------------------------------------------------- |
| **A — cheap, in-DB**                | the new table is a pure projection of data **already in ClickHouse** (e.g. `arrayJoin` of an existing column) | minutes–hours, one query                   | `INSERT … SELECT` on the box. **No re-parse.**      |
| **B — expensive, from-S3 re-parse** | the new column/grain exists **only in the ledger XDR** (multi-leg ops, before/after images, per-op fan-out)   | ~1 TB of XDR streamed, ~day, disk-governed | `s5cmd` pre-fetch + `backfill-runner run --reindex` |

`operation_pools` was **A** (`pool_ids` was already a column on
`operations_appearances`). `operation_asset_appearances` was **B** (classic
multi-leg asset data lives only in XDR). **If your table is flavour A, stop here
and use [§5](#5-flavour-a--cheap-in-db-backfill-no-re-parse).** The rest of this
runbook is flavour B.

---

## 1. The box (verify current — these are 2026-07 values)

| Thing          | Value                                                                                                                                                                                                          |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| SSH alias      | `sorban-prod` → `deploy@ch-prod-01`, user `deploy`, home `/home/deploy`                                                                                                                                        |
| SSH key        | `~/.ssh/sorban-prod_ed25519` (passphrase). **`ssh-add -l` first** — never fire blind attempts (fail2ban).                                                                                                      |
| ClickHouse     | docker container `app-clickhouse-1`, reachable at `http://localhost:8123`. Client: `docker exec -i app-clickhouse-1 clickhouse-client` (default user; password in the container/compose env, `/srv/app/.env`). |
| Root disk      | `/dev/md1` ≈ 1.8 TB — **the binding constraint** (see [§7](#7-phase-e--disk--merge-governance-mandatory-for---reindex)).                                                                                       |
| CH memory cap  | `max_memory_usage` 6 GB (`users.d/timeouts.xml`); OPTIMIZE/heavy queries need `--receive_timeout 3600`.                                                                                                        |
| Missing on box | **no repo, no cargo, no `aws`/`s5cmd` by default.** Binary is cross-built on the laptop and `scp`'d; `s5cmd` + `aws` live user-local under `/home/deploy` (reused across backfills).                           |

## 2. Preconditions

- [ ] **`backfill-runner` supports `--reindex`** ([lore-0379], PR #333). Without
      it, `run` skips ledgers already in the `ledgers` table (resume semantics),
      so re-parsing already-ingested history writes **0 rows** — a silent no-op.
- [ ] **`s5cmd` present on the box** (`/home/deploy/s5cmd`). Public bucket →
      `--no-sign-request`, region `us-east-2`.
- [ ] **Target table created manually** on prod — `init.sql` is
      fresh-install-only (`CREATE … IF NOT EXISTS` never re-runs on the live DB).
- [ ] **Pre-backfill `BACKUP`** taken ([§4](#4-phase-b--create-the-table--safety-backup-box)).
- [ ] **Disk headroom** — flavour B rewrites **every** table (see §7); the 2026-07
      run peaked at 92 % of 1.8 TB. Do not start under ~300 GB free.

---

## 3. Phase A — build & ship the binary (laptop)

The box has no toolchain; cross-compile to its glibc and `scp`:

```bash
# LAPTOP (macOS ARM) — one-time tooling
brew install zig
cargo install cargo-zigbuild
rustup target add x86_64-unknown-linux-gnu

# LAPTOP — cross-compile to glibc-2.31 x86_64 Linux, then ship
cd ~/Developer/RumbleFish/soroban-block-explorer
ulimit -n 65536
cargo zigbuild --release -p backfill-runner --bin backfill-runner \
  --target x86_64-unknown-linux-gnu.2.31
scp target/x86_64-unknown-linux-gnu/release/backfill-runner deploy@ch-prod-01:~/backfill-runner
```

```bash
# BOX — smoke
chmod +x ~/backfill-runner
./backfill-runner --version
./backfill-runner run --help | grep -- --reindex   # MUST be present
```

> ⚠️ If `--reindex` is missing, you shipped a pre-flag binary — rebuild and
> re-`scp`. (This exact miss happened in the 2026-07 run; the first backfill
> wrote 0 rows.)

---

## 4. Phase B — create the table + safety backup (box)

```bash
# BOX — create the target table (example: operation_asset_appearances, 0359)
docker exec -i app-clickhouse-1 clickhouse-client --multiquery <<'SQL'
CREATE TABLE IF NOT EXISTS operation_asset_appearances (
    asset_id        Int64,
    ledger_sequence Int64,
    transaction_id  Int64
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (asset_id, ledger_sequence, transaction_id);
SQL
```

```bash
# BOX — pre-backfill snapshot (ASYNC dodges the 300 s client receive_timeout)
docker exec app-clickhouse-1 clickhouse-client -q \
  "BACKUP DATABASE default TO Disk('backups', 'snapshot_pre_<task>_backfill_$(date +%Y%m%d)') ASYNC"
docker exec app-clickhouse-1 clickhouse-client -q \
  "SELECT name, status, error, formatReadableSize(total_size) FROM system.backups ORDER BY start_time DESC LIMIT 1"
```

Author a reusable env file so every worker/tmux pane shares config:

```bash
# BOX — ~/meta.env  (source with:  set -a; source ~/meta.env; set +a)
export CLICKHOUSE_URL=http://localhost:8123
export CLICKHOUSE_USER=default
export CLICKHOUSE_PASSWORD='<PW>'        # from /srv/app/.env — do not commit
export CLICKHOUSE_DATABASE=default
export BIN=/home/deploy/backfill-runner
export S5CMD=/home/deploy/s5cmd
```

---

## 5. Flavour A — cheap in-DB backfill (no re-parse)

If the grain already exists in ClickHouse, skip Phases A/D entirely — one query:

```bash
# BOX — operation_pools (0365): arrayJoin an existing column, RMT-dedup
docker exec -i app-clickhouse-1 clickhouse-client --receive_timeout 3600 --multiquery <<'SQL' 2>&1 | tee /tmp/bf-pools.log
INSERT INTO operation_pools
SELECT arrayJoin(pool_ids) AS pool_id, ledger_sequence, transaction_id
FROM operations_appearances
SETTINGS max_execution_time = 0;
SQL
```

Done when the log is clean and `count()` is stable. (2026-07: ~363 M rows.)

---

## 6. Phase D — s5cmd pre-fetch + `run --reindex` (the core loop)

### 6.1 Partition / S3 layout (the archive)

Public bucket `s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/`, one
`.xdr.zst` per ledger, grouped into **64 000-ledger** folders
(see [Stellar Pubnet Ledger Archive](../../lore/3-wiki/stellar-pubnet-ledger-archive.md)).
Folder and file names use a **reverse-hex** key `HEX = %08X(u32::MAX − ledger)`:

```
<HEX(f0)>--<f0>-<f1>/                       # folder, f0 = 64k-aligned start
<HEX(seq)>--<seq>.xdr.zst                    # one file per ledger
```

Self-check the math against the archive doc before trusting a script:

```bash
for s in 62016000 50560000 50432000; do printf '%d -> %08X\n' "$s" $((4294967295 - s)); done
# expect: 62016000 -> FC4DB5FF ; 50560000 -> FCFC83FF ; 50432000 -> FCFE77FF
```

`backfill-runner` normally fetches each folder itself via `aws s3 sync`
(`sync.rs`); pre-filling the temp-dir with **s5cmd** (parallel fan-out) makes its
fast-path skip that sync. The local layout must match exactly:
`<temp-dir>/<HEX>--<f0>-<f1>/` holding all 64 000 `.xdr.zst` files.

### 6.2 The worker script — `bf-loop16.sh`

One worker owns a ledger range. Per 64k folder: s5cmd-fetch it, then run the
binary over it in **16 000-ledger sub-windows** (the "channel closed" fix, see
§10), keeping the fetched folder (`--keep-partitions`) across the 4 sub-runs and
`rm`-ing it only after. A watermark file makes it resumable.

```bash
#!/usr/bin/env bash
# BOX — ~/bf-loop16.sh <start> <end> <watermark-file>
set -uo pipefail
START=$1; END=$2; WM=$3; F=64000; P=16000
DATA="${DATA:?}"; BIN="${BIN:?}"; S5CMD="${S5CMD:-s5cmd}"
: "${CLICKHOUSE_URL:?}"
BUCKET="s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet"
export AWS_REGION=us-east-2 AWS_DEFAULT_REGION=us-east-2
export STELLAR_NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
mkdir -p "$DATA"
fstart=$(( START - (START % F) ))
for (( f0=fstart; f0<=END; f0+=F )); do
  f1=$(( f0+F-1 )); lo=$(( f0>START?f0:START )); hi=$(( f1<END?f1:END ))
  [ -s "$WM" ] && [ "$(cat "$WM")" -ge "$hi" ] && continue          # already done
  folder="$(printf '%08X' $((4294967295-f0)))--${f0}-${f1}"
  dir="$DATA/$folder"; mkdir -p "$dir"; list="$(mktemp)"
  for (( s=f0; s<=f1; s++ )); do
    printf 'cp "%s/%s/%08X--%d.xdr.zst" "%s/"\n' "$BUCKET" "$folder" $((4294967295-s)) "$s" "$dir"
  done > "$list"
  for t in 1 2 3; do echo "[$(date +%F\ %T)] s5cmd $f0..$f1 (t$t)"; "$S5CMD" --log error --no-sign-request run "$list" && break; sleep 15; done
  rm -f "$list"
  for (( slo=lo; slo<=hi; slo+=P )); do                            # 16k RUN sub-windows
    shi=$(( slo+P-1 )); [ "$shi" -gt "$hi" ] && shi=$hi
    [ -s "$WM" ] && [ "$(cat "$WM")" -ge "$shi" ] && continue
    for t in 1 2 3; do
      echo "[$(date +%F\ %T)] run $slo..$shi (t$t)"
      "$BIN" --clickhouse-url "$CLICKHOUSE_URL" --temp-dir "$DATA" --keep-partitions \
        run --reindex --start "$slo" --end "$shi" && { echo "$shi" > "$WM"; break; }
      sleep 30
    done
  done
  rm -rf "$dir"                                                    # after all sub-windows
done
echo "[$(date +%F\ %T)] DONE $START..$END"
```

### 6.3 Fan-out launcher

```bash
# BOX — N workers over the full range, each with its own DATA dir + watermark
set -a; source ~/meta.env; set +a
rm -rf ~/bf-359; mkdir -p ~/bf-359
S=50457424; E=63460100; N=6            # start modest — see §6.4
STEP=$(( (E-S)/N ))
for i in $(seq 0 $((N-1))); do
  Si=$(( S + i*STEP )); Ei=$(( i==N-1 ? E : S + (i+1)*STEP ))
  DATA=~/bf-359/w$i nohup ~/bf-loop16.sh $Si $Ei ~/bf-359/wm$i.txt > ~/bf-359/w$i.log 2>&1 &
done
jobs
```

Prove `--reindex` + idempotency on one small slice first:

```bash
# BOX
set -a; source ~/meta.env; set +a
"$BIN" -v --clickhouse-url "$CLICKHOUSE_URL" --temp-dir ~/bf-dbg run --reindex --start 50457424 --end 50457999
docker exec app-clickhouse-1 clickhouse-client -q \
 "SELECT count() c, uniqExact((asset_id,ledger_sequence,transaction_id)) k
  FROM operation_asset_appearances WHERE ledger_sequence BETWEEN 50457424 AND 50457999"
# re-run the same slice → k must stay identical (RMT idempotent). c may shrink toward k on merge.
```

### 6.4 Worker count

The bottleneck is **ClickHouse insert/persist, not S3 or CPU.** In 2026-07 on
24 cores, 9 and 13 workers were **no faster** than 6, and >6 only worsened disk
pressure. **Start at ~6.** The sparse early era and dense tip are very uneven
(a tip 64k slice ≈ 5400 s, persist-bound) — if the tip starves, re-slice it into
finer sub-ranges rather than adding global workers.

---

## 7. Phase E — disk & merge governance (mandatory for `--reindex`)

**`--reindex` re-parses the full ledger, so it rewrites _all_ ~22 streaming
tables — the target table is only ~10 % of the write; the rest is unavoidable
side-effect.** Parts explode faster than background merges collapse them, and
root disk fills. You must actively govern it:

```bash
# BOX — free reclaim: system logs regrow, safe to TRUNCATE (recovered ~60 GB in 2026-07)
for t in text_log processors_profile_log query_log; do
  docker exec app-clickhouse-1 clickhouse-client -q "TRUNCATE TABLE system.$t"
done

# BOX — collapse RMT dups on "done" partitions (partition = intDiv(ledger_sequence, 500000))
for p in $(seq 100 126); do
  for t in operation_asset_appearances transactions transaction_hash_index soroban_events \
           operations_appearances transaction_participants liquidity_pool_snapshots; do
    docker exec app-clickhouse-1 clickhouse-client --receive_timeout 3600 \
      -q "OPTIMIZE TABLE $t PARTITION ID '$p' FINAL SETTINGS optimize_skip_merged_partitions=1"
  done
  printf 'part %s -> ' "$p"; df -h / | tail -1
done
```

For an unattended overnight run this was automated into a **supervisor** (`tmux`
pane) that: resumes paused sub-workers up to a `TARGET` worker count, sheds
workers when free disk drops, reclaims via the OPTIMIZE loop above, and on
completion runs a final full OPTIMIZE. If you write one, learn from §10's
supervisor bugs: **fail-safe** on disk-parse errors, **don't swallow** OPTIMIZE
stderr (count failures, gate "complete" on zero), take the OPTIMIZE partition
ceiling from the **backfill END**, not the live tip.

---

## 8. Phase F — monitor (laptop → box)

```bash
# LAPTOP  (ssh-add -l first)
ssh -o BatchMode=yes -o ConnectTimeout=12 sorban-prod \
  "df -h / | tail -1; pgrep -af 'bf-loop' | wc -l; tail -8 ~/bf-359/supervisor.log"
# per-worker watermark progress
ssh sorban-prod 'for f in ~/bf-359/wm*.txt; do echo "$f -> $(cat "$f")"; done'
```

---

## 9. Phase G — validate (MANDATORY gate — do not skip, do not drop the snapshot before this)

**Reaching every watermark is _not_ proof of coverage.** Before trusting the
table or dropping the pre-backfill snapshot:

1. **Gap scan** — every ledger in `[start,end]` present, no holes:
   ```bash
   docker exec app-clickhouse-1 clickhouse-client -q "
   SELECT count() FROM (
     SELECT ledger_sequence FROM operation_asset_appearances
     WHERE ledger_sequence BETWEEN 50457424 AND 63460100 GROUP BY ledger_sequence)"
   # compare against the ledgers actually closed in-range (ledgers table / Horizon)
   ```
2. **Sample assets vs Horizon / stellar.expert** — incl. native + a type-3 token
   — via the `/compare-with-stellar-api` skill. List + all detail variants.
3. **Read-in-order** — `EXPLAIN indexes=1` / `read_rows` on a hot asset confirms
   the PK-prefix seek (the whole point of the table).

Only after G passes: run the final `OPTIMIZE … FINAL`, then drop the pre-backfill
snapshot.

---

## 10. Troubleshooting (each gotcha hit in 2026-07 → its prevention)

| Symptom                                         | Cause                                                                     | Fix                                                                                                          |
| ----------------------------------------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| Backfill writes **0 rows** ("nothing to do")    | `run` resume-skips ledgers already in `ledgers`                           | Ship a `--reindex` binary (§3). This is the entire reason the flag exists.                                   |
| `error: unexpected argument --target`           | box binary has no `--target` flag (that was the docker/Lambda path)       | Use `--clickhouse-url $CLICKHOUSE_URL --temp-dir $DATA`, no `--target`.                                      |
| Runner fail-fasts: `aws` not found              | `aws` not on PATH inside `tmux`/`nohup`                                   | `echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc`. Mooted once s5cmd pre-fetch fast-paths the sync. |
| `channel closed` mid-run on dense partitions    | a single 64k `run` exceeds the CH insert channel timeout (risks **gaps**) | 16k RUN sub-windows + `--keep-partitions` (already in `bf-loop16.sh`).                                       |
| Root disk fills toward 100 %                    | `--reindex` rewrites all ~22 tables; parts outrun merges                  | TRUNCATE system logs + OPTIMIZE loop (§7); drop workers to ~6.                                               |
| Sparse early workers starve                     | uneven era density; tip workers hog inserts                               | re-slice the dense tip into finer sub-ranges; don't add global workers.                                      |
| `BIN: parameter null or not set` on resume      | worker relaunched without env                                             | `set -a; source ~/meta.env; set +a` before resuming.                                                         |
| Supervisor reports "COMPLETE" but merges failed | OPTIMIZE stderr swallowed by `2>&1`                                       | count failures into a `FAILS` var + `optimize.err`; gate success on `FAILS==0`.                              |

See also [`0225_backfill_crash_recovery.md`](0225_backfill_crash_recovery.md) for
orphan-row cleanup when a `run` panics mid-partition (streaming tables keep
partial rows; `ledgers` doesn't — commit-marker pattern).

---

## 11. Cleanup

- `rm -rf ~/bf-<task>` (temp-dir XDR + logs) once validated.
- If you raised the CH memory cap for OPTIMIZE, revert `users.d/timeouts.xml` +
  `docker restart app-clickhouse-1`.
- Drop the pre-backfill snapshot **only after Phase G passes**.

---

## Appendix — worked example: `operation_asset_appearances` (0359/0379) + `operation_pools` (0365), 2026-07-13→16

The run this runbook is generalised from. Session "Backfill planning and
execution" (PR #328 was the sibling 0385 MV; the `--reindex` code is PR #333).

**What ran, in order:**

1. **Code (laptop):** add `run --reindex` ([0379], feat/0379 → PR #333); fix
   `repair_tier1` stale `name` column (lore-0388). `cargo zigbuild` → `scp` binary
   to `deploy@ch-prod-01`. First binary predated the flag → 0 rows → rebuilt.
2. **Box:** `CREATE TABLE operation_asset_appearances` + `operation_pools`;
   `BACKUP … ASYNC` (`snapshot_pre_0359_backfill_20260713`); `~/meta.env`.
3. **`operation_pools` (0365):** `INSERT … SELECT arrayJoin(pool_ids) …` — flavour
   A, no re-parse → ~**363 M** deduped rows. **DONE** same day.
4. **`operation_asset_appearances` (0359):** flavour B. `bf-loop16.sh` fan-out
   over **50 457 424 → 63 460 100** (~13.0 M ledgers), s5cmd pre-fetch +
   `run --reindex`, 16k sub-windows. Workers throttled 16 → **6** (CH-insert
   ceiling); dense tip re-sliced into 13 finer sub-ranges; governed overnight by
   a supervisor `tmux` pane (resume + disk-shed + OPTIMIZE).

**Outcome (as of 2026-07-16 ~07:36 UTC):**

- **Re-index COMPLETE** — all 13 sub-ranges at 100 %, continuous coverage
  (`sum(END−START)` matched); supervisor auto-ran `FULL OPTIMIZE 100..126` over 7
  tables; `optimize.err = 0`; disk `/dev/md1` 92 % (135 GB free).
- **Throughput** ≈ 127–150 k ledgers/hr at 6 workers (persist-bound; 9/13 workers
  no faster).
- **`operation_pools` DONE**; `accounts_recent` MV (0385) landed as a sibling.

**Deferred to a follow-up session (NOT done — do not treat the table as verified):**

- **Phase G validation** — gap-scan + Horizon/stellar.expert cross-check
  (`/compare-with-stellar-api`). **No final verified `operation_asset_appearances`
  row count was produced.** Watermarks reached ≠ coverage proven.
- **Phase 3** — `repair_tier1` (after PR #336) → indexer STOP →
  `repair-tier1 --dry-run` → `repair-tier1` → `nft-reclassify` → validate → START.

> Supervisor hardening note: the unattended governor script (`bf-supervisor.sh`)
> was reviewed by an adversarial multi-agent pass before the overnight run; the
> fixes it forced are captured as prevention in [§10](#10-troubleshooting-each-gotcha-hit-in-2026-07--its-prevention) and [§7](#7-phase-e--disk--merge-governance-mandatory-for---reindex).

[0359]: ../../lore/1-tasks/archive/0359_FEATURE_asset-participation-index-remodel/README.md
[0379]: ../../lore/1-tasks/backlog/0379_OPS_deploy-backfill-operation-asset-appearances.md
