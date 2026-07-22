# Runbook: Soroban era backfill (2/5 range) on a fresh machine

> **Partly retired — task 0392 (2026-07-22).** Steps touching `nfts_pending` /
> `nft_ownership_pending` (and `backfill-runner nft-reclassify`) no longer apply:
> those tables and that subcommand are gone, and NFT visibility is a read-time
> filter on the contract's verdict
> ([ADR 0053](../../lore/2-adrs/0053_nft-visibility-as-read-time-verdict-filter.md)).
> The rest of this runbook stands.

**Goal:** Index 2/5 of the Soroban era (~4.6 M ledgers, ~73 partitions,
~80 hours) to a local ClickHouse on a clean machine. Ledgers
`50,457,424` (protocol-20 activation, Feb 20 2024) through `55,103,999`
(end of partition 860).

**Target machine prereqs:**

- Linux or macOS
- 4+ CPU cores, 16 GB+ RAM
- **2 TB+ disk** for S3 scratch + CH data
- Internet (S3 + Soroban RPC connectivity)
- Docker + Docker Compose v2
- Rust toolchain (latest stable)
- `aws` CLI (for S3 sync, called by `backfill-runner`)
- Python 3.12+ (optional, for verification scripts)

> **Why 2/5, why these bounds:**
> Soroban era total = 50,457,424 → ~62,000,000 (~11.6 M ledgers, ~180
> partitions). 2/5 ≈ 73 partitions × 64,000 ledgers each. Start at the
> literal protocol-20 activation ledger so the data covers every Soroban
> contract ever deployed; end at a partition boundary so the last
> partition is fully indexed.

---

## 1. Install toolchain

### macOS

```bash
# Homebrew prerequisites (if missing)
brew install rustup docker docker-compose awscli python@3.12

# Rust toolchain
rustup default stable

# Docker Desktop must be running before next steps
```

### Ubuntu / Debian

```bash
sudo apt update
sudo apt install -y curl build-essential git pkg-config libssl-dev awscli python3.12 python3.12-venv

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# Docker Engine (skip if already installed)
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker $USER
# (log out + back in for group to take effect)
```

Verify:

```bash
rustc --version    # rustc 1.7x.x+
docker --version   # Docker 24.x+
docker compose version
aws --version
```

---

## 2. Clone repo

```bash
mkdir -p ~/Desktop/soroban && cd ~/Desktop/soroban
git clone https://github.com/rumblefishdev/soroban-block-explorer.git
cd soroban-block-explorer

# Use the branch that has the bootstrap subcommand + watermark fix
# (after PR #189 merges, switch to `develop` instead)
git checkout feat/0214-bootstrap-subcommand
```

---

## 3. Build backfill-runner (release mode)

```bash
cargo build --release -p backfill-runner

# Verify binary
./target/release/backfill-runner --help | head -20
./target/release/backfill-runner help    # confirms `bootstrap` subcommand exists
```

Expected: lists `run`, `status`, `bootstrap` subcommands.

---

## 4. Start ClickHouse + apply schema

```bash
# Start CH container (defaults to user=default, pass=clickhouse, db=default)
docker compose up -d clickhouse

# Wait for CH to be healthy (~5-10s)
until docker compose exec -T clickhouse clickhouse-client \
    --user=default --password=clickhouse \
    --query="SELECT 1" >/dev/null 2>&1; do
  echo "Waiting for ClickHouse to be ready..."
  sleep 2
done
echo "ClickHouse is ready."

# Apply canonical schema (ADR 0044)
docker compose run --rm db-clickhouse-init

# Verify tables exist
docker compose exec -T clickhouse clickhouse-client \
    --user=default --password=clickhouse --database=default \
    --query="SHOW TABLES" | wc -l
# Expected: 20 (per ADR 0044 init.sql)
```

---

## 5. Setup verification helpers (optional, but useful for post-backfill QA)

```bash
# Python venv for py-stellar-sdk (used by verification scripts)
python3 -m venv ~/.local/venvs/stellar-sdk
~/.local/venvs/stellar-sdk/bin/pip install --upgrade pip
~/.local/venvs/stellar-sdk/bin/pip install stellar-sdk==14.0.0
~/.local/venvs/stellar-sdk/bin/python -c "import stellar_sdk; print(stellar_sdk.__version__)"
# Expected: 14.0.0
```

---

## 6. Run the backfill (2/5 of Soroban era)

```bash
CLICKHOUSE_USER=default \
CLICKHOUSE_PASSWORD=clickhouse \
CLICKHOUSE_DATABASE=default \
STELLAR_NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015" \
./target/release/backfill-runner \
    --target clickhouse \
    --clickhouse-url "http://127.0.0.1:8123" \
    --soroban-rpc-url "https://mainnet.sorobanrpc.com" \
    run \
    --start 50457424 \
    --end 55103999
```

**What this does:**

- Downloads 73 S3 partitions (~860 GB total, pipelined: partition N+1
  syncs while partition N indexes)
- Parses + persists ~4.6 M ledgers into CH
- Runs the per-window bootstrap RPC pass after each partition (top-ups
  `sequence_number=0` skeletons via `getLedgerEntries`)
- Idempotent — safe to interrupt with Ctrl-C and re-run; RMT dedupes;
  `backfill-runner` has resume logic

**ETA:** ~60-80 hours depending on:

- S3 download bandwidth (single-IP throughput cap)
- Soroban RPC rate limiting (50 RPS public limit)
- Local CPU (parse phase is single-threaded per partition)

**Run in `tmux` / `screen` / `nohup`:** session may close; output goes
to stdout, no log file by default. Either:

```bash
# Option A — tmux
sudo apt install tmux        # or: brew install tmux
tmux new-session -s backfill
# (paste run command, detach with Ctrl-B then D)
# Reattach later: tmux attach -t backfill

# Option B — capture to log file
... run command ... 2>&1 | tee backfill-2of5.log
```

---

## 7. Monitor progress (in a separate terminal)

```bash
# Poll every 5 min — partition progress + skeleton rate
watch -n 300 'docker compose exec -T clickhouse clickhouse-client \
    --user=default --password=clickhouse --database=default --query="
SELECT
    count() AS tx_total,
    min(ledger_sequence) AS min_l,
    max(ledger_sequence) AS max_l,
    max(ledger_sequence) - 50457423 AS ledgers_done,
    round((max(ledger_sequence) - 50457423) * 100.0 / 4646576, 2) AS pct_done
FROM transactions FINAL FORMAT Vertical"'
```

Accounts + skeleton rate (separate poll, less frequent):

```bash
docker compose exec -T clickhouse clickhouse-client \
    --user=default --password=clickhouse --database=default --query="
SELECT
    count() AS accounts_total,
    countIf(sequence_number=0) AS skeletons,
    round(countIf(sequence_number=0)*100.0/count(), 2) AS skeleton_pct
FROM accounts FINAL FORMAT Vertical"
```

Disk space:

```bash
watch -n 300 'docker compose exec -T clickhouse du -sh /var/lib/clickhouse/data 2>/dev/null; df -h ~/Desktop/soroban'
```

---

## 8. Post-backfill verification

After `run` completes (or you've decided to call it done at the
current progress):

### 8a. Final bootstrap top-up pass

The per-window bootstrap may have missed accounts due to RPC throttling
or partial commits. Run one final pass over the full range — the
auto-watermark fix in `bootstrap.rs` guarantees the snapshot stamp wins
all RMT races:

```bash
CLICKHOUSE_USER=default CLICKHOUSE_PASSWORD=clickhouse CLICKHOUSE_DATABASE=default \
./target/release/backfill-runner \
    --target clickhouse \
    --clickhouse-url "http://127.0.0.1:8123" \
    --soroban-rpc-url "https://mainnet.sorobanrpc.com" \
    bootstrap \
    --start 50457424 --end 55103999
```

Expected: `bootstrap completed: discovered=N fetched=M staged=M rpc_errors=0`.

If rate-limited (Cloudflare 1015): wait 5-15 min, retry. Or use alt RPC
`https://soroban-rpc.mainnet.stellar.gateway.fm`.

### 8b. Drain 0221 SAC leak (~30 min)

Follow [`docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md`](./0221_ch_drain_sac_from_nfts_pending.md).
Quick form:

```bash
docker compose exec -T clickhouse clickhouse-client --user=default --password=clickhouse --database=default --query="
ALTER TABLE nfts_pending
DELETE WHERE contract_id IN (
    SELECT id FROM soroban_contracts FINAL
    WHERE is_sac = true OR contract_type IN (0, 3)
)"

# Wait for mutation
docker compose exec -T clickhouse clickhouse-client --user=default --password=clickhouse --database=default --query="
SELECT is_done FROM system.mutations WHERE table='nfts_pending' ORDER BY create_time DESC LIMIT 1"

# Compact
docker compose exec -T clickhouse clickhouse-client --user=default --password=clickhouse --database=default --query="
OPTIMIZE TABLE nfts_pending FINAL"
```

### 8c. Measure final state

```bash
docker compose exec -T clickhouse clickhouse-client --user=default --password=clickhouse --database=default --query="
SELECT
    (SELECT count() FROM accounts FINAL) AS accounts,
    (SELECT countIf(sequence_number=0) FROM accounts FINAL) AS skeletons,
    (SELECT round(countIf(sequence_number=0) * 100.0 / count(), 2) FROM accounts FINAL) AS skeleton_pct,
    (SELECT count() FROM assets FINAL WHERE asset_type=1) AS classic_credits,
    (SELECT count() FROM soroban_contracts FINAL) AS contracts,
    (SELECT countIf(is_sac=true) FROM soroban_contracts FINAL) AS sac_contracts,
    (SELECT count() FROM nfts FINAL) AS nfts_hot,
    (SELECT count() FROM nfts_pending FINAL) AS nfts_pending
FORMAT Vertical"
```

**Expected ranges (very approximate, scale-dependent):**

| Metric          | 2/5 range guess                                     |
| --------------- | --------------------------------------------------- |
| accounts        | 5-15 M                                              |
| skeleton_pct    | <1 % after bootstrap                                |
| classic_credits | ~100k-200k                                          |
| sac_contracts   | ~50k-150k                                           |
| nfts_hot        | 0 (false positives reverted in 0118)                |
| nfts_pending    | scale-proportional to total NFT-candidate transfers |

---

## 9. Troubleshooting

### Backfill panics mid-partition

Symptom:

```
panic at ingest.rs:NNN: ledger file missing post-sync: partition=NN seq=NN
```

Cause: AWS S3 archive lag (recent partitions only). 2/5 range is
historical (~2024), so this shouldn't happen — if it does, follow
[`docs/runbooks/0225_backfill_crash_recovery.md`](./0225_backfill_crash_recovery.md).

Quick recovery:

```bash
# Re-run with same parameters; backfill-runner has resume logic
# (skips already-committed partitions, restarts the failed one)
... same run command ...
```

### Soroban RPC rate limiting

Symptom:

```
RPC fetch failed; bailing out err=rpc http status 429 body: error code: 1015
```

Cause: Cloudflare WAF on `mainnet.sorobanrpc.com` after sustained RPC
load.

Fix options:

- Wait 5-15 min for the rate limit to expire, retry
- Switch RPC: `--soroban-rpc-url https://soroban-rpc.mainnet.stellar.gateway.fm`
- Reduce concurrency: edit `crates/backfill-runner/src/rpc_snapshot.rs::DEFAULT_CONCURRENCY` (currently 4) → rebuild

### ClickHouse OOMs / panics

Symptom: container exits, `docker compose ps` shows stopped.

Cause: `OPTIMIZE TABLE` or `ALTER … DELETE` on large RMT exceeds the
container's RAM limit.

Fix:

- Increase Docker memory limit (Docker Desktop → Resources → Memory)
- Add `--max_memory_usage_for_user=...` to clickhouse-client commands
- Run `OPTIMIZE` on individual partitions instead of full table

### `--soroban-rpc-url` was missing

Symptom: high skeleton percentage after backfill (e.g. 15-50 %).

Fix: re-run bootstrap with the flag (Step 8a). The auto-watermark
fix makes this idempotent and safe to re-run.

---

## 10. Quick reference — concrete numbers

| Param            | Value                                                      |
| ---------------- | ---------------------------------------------------------- |
| Start ledger     | 50,457,424 (literal Soroban genesis)                       |
| End ledger       | 55,103,999 (partition 860 end)                             |
| Ledgers indexed  | 4,646,576                                                  |
| Partitions       | 73                                                         |
| S3 bandwidth     | ~860 GB (incl. ~4.5 GB pre-Soroban waste in partition 788) |
| ETA              | 60-80 hours                                                |
| Final disk usage | ~250-400 GB CH data (estimate)                             |

---

## 11. After 2/5 — extending to 5/5

Same command, different end ledger:

```bash
--start 55104000 --end 59750999    # next 73 partitions (5/5 = 3/5 done)
--start 59751000 --end <current_max>   # remainder up to tip
```

Each invocation is independent (resume logic handles cross-window
boundaries). RMT dedup means re-running an overlapping range is a no-op.
