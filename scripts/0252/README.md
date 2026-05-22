# Task 0252 — Phase B compare scripts

Per-endpoint CH ↔ Horizon / stellar.expert parity validation. Run on
the Hetzner box (`sorban-prod`) where CH is local; Horizon is remote.

## Layout

```
scripts/0252/
├── sample_pools.sh         # one-shot sample generators (ledgers,
│                             accounts, assets, pools, contracts)
├── common.py               # CH + Horizon clients, diff helpers, TSV
├── compare_e03.py          # E03 /transactions/:hash  (pilot endpoint)
├── compare_e02.py          # E02 /transactions list   (TBD)
├── compare_e04.py          # E04 /ledgers list        (TBD)
├── compare_e05.py          # E05 /ledgers/:sequence   (TBD)
├── compare_e06.py          # E06 /accounts/:id        (TBD)
├── compare_e07.py          # E07 /accounts/:id/tx     (TBD)
├── compare_e09.py          # E09 /assets/:id          (TBD)
├── compare_e18.py          # E18 /liquidity-pools     (TBD)
├── compare_e19.py          # E19 /liquidity-pools/:id (TBD)
└── compare_e20.py          # E20 /liquidity-pools/:id/tx (TBD)
```

## Setup on Hetzner

```bash
# One-time: install Python deps in a venv
ssh sorban-prod 'sudo apt install -y python3-venv
python3 -m venv ~/sbe-0252-venv
~/sbe-0252-venv/bin/pip install requests'

# Deploy scripts
WT=/Volumes/Extreme\ SSD\ 2TB/claude-code-worktrees/soroban-block-explorer/feat-0252-endpoint-parity
scp -r "$WT/scripts/0252" sorban-prod:/tmp/
ssh sorban-prod 'chmod +x /tmp/0252/sample_pools.sh /tmp/0252/*.py'
```

## Run sequence

```bash
ssh sorban-prod
# 1. Generate sample pools (~30 sec total)
/tmp/0252/sample_pools.sh

# 2. Pilot one endpoint to validate (~5 min)
SBE_PILOT_LIMIT=100 ~/sbe-0252-venv/bin/python3 /tmp/0252/compare_e03.py

# 3. Inspect pilot result
cat /tmp/sbe-artifacts/0252/phase_b_e03_summary.json
column -t -s $'\t' < /tmp/sbe-artifacts/0252/phase_b_e03.tsv | head -20

# 4. If pilot green → full run, background
nohup ~/sbe-0252-venv/bin/python3 /tmp/0252/compare_e03.py \
  > /tmp/sbe-artifacts/0252/e03_run.log 2>&1 &
```

## Resumability

Every script reads its TSV output before starting and skips keys
already present in column 2. So a crashed run can be restarted with
the same command — finished work is preserved.

## Output

- `/tmp/sbe-artifacts/0252/phase_b_e<NN>.tsv` — one row per compared key
  with `pass / tolerance / fail` per-field counters + note column.
- `/tmp/sbe-artifacts/0252/phase_b_e<NN>_summary.json` — aggregated
  per-field totals + elapsed wall time.
- `/tmp/sbe-artifacts/0252/diffs/E<NN>/<key>.json` — full CH ↔ Horizon
  record dump for any key with field mismatches (offline analysis).

Phase E aggregator (TBD) reads the per-endpoint TSVs and emits the
final Markdown artifact per the Reporting Shape in task 0252 README.

## Environment overrides

| Var                 | Default                       | Purpose                      |
| ------------------- | ----------------------------- | ---------------------------- |
| `SBE_CH_CONTAINER`  | `app-clickhouse-1`            | docker container name        |
| `SBE_CH_USER`       | `default`                     | CH user                      |
| `SBE_CH_DB`         | `default`                     | CH database                  |
| `SBE_OUT_DIR`       | `/tmp/sbe-artifacts/0252`     | output + diff dir            |
| `SBE_SAMPLE_DIR`    | same as OUT_DIR               | sample input dir             |
| `HORIZON_BASE`      | `https://horizon.stellar.org` | Horizon REST root            |
| `SBE_HORIZON_DELAY` | `0.35` (sec)                  | inter-request sleep          |
| `SBE_PILOT_LIMIT`   | `0` (full run)                | cap input to first N samples |
