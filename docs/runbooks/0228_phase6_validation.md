# Runbook: 0228 Phase 6 — End-to-end validation of merged Hetzner CH

> **Partly retired — task 0392 (2026-07-22).** Steps touching `nfts_pending` /
> `nft_ownership_pending` (and `backfill-runner nft-reclassify`) no longer apply:
> those tables and that subcommand are gone, and NFT visibility is a read-time
> filter on the contract's verdict
> ([ADR 0053](../../lore/2-adrs/0053_nft-membership-decided-at-write-time-from-wasm.md)).
> The rest of this runbook stands.

**Task:** [0228 — parallel-backfill merge into Hetzner CH](../../lore/1-tasks/active/0228_FEATURE_parallel-backfill-merge-and-validation/README.md)
**Target:** ClickHouse production (`ch-prod-01`, container `app-clickhouse-1`)
**Idempotent:** yes (read-only checks)
**Frequency:** one-shot per merge; rerun after any Phase 5 fix

---

## When to run

Run **after** all Phase 5 repair subcommands have completed on Hetzner:

1. `backfill-runner repair-tier1`
2. `OPTIMIZE TABLE soroban_contracts FINAL` (manual, deferred from attach
   script per task 0228 design)
3. `backfill-runner asset-aggregates`
4. `backfill-runner nft-reclassify`

This runbook implements the acceptance criteria for Phase 6 in
[task 0228](../../lore/1-tasks/active/0228_FEATURE_parallel-backfill-merge-and-validation/README.md):

- Ledger continuity from `50,457,424` to `L_last_closed` with zero gaps
- All 19 CH tables + 1 dictionary populated
- No-`FINAL`-at-query-time invariant
- Tier-1 column rebuild verified against fact-table sources
- Sample-compare against Horizon / stellar.expert ≤ 0.01 % mismatch on
  1000 stratified ledgers

## What it does

Read-only validation pass against the merged Hetzner CH. Organised into
six tiers from cheapest to most expensive; **stop-on-failure** between
tiers so you fix problems before they cascade.

| Tier | Checks                               | Wall-clock |
| ---- | ------------------------------------ | ---------- |
| 1    | Sanity (rows, parts, dict, no-FINAL) | 5–10 min   |
| 2    | Tier-1 column rebuild correctness    | 10–20 min  |
| 3    | Row-count parity vs worker baselines | 15–30 min  |
| 4    | Skeleton %, orphans, per-ledger tx   | 5–10 min   |
| 5    | Sample-compare against Horizon       | 1–2 h      |
| 6    | Cross-check via repo scripts         | ad-hoc     |

`L_last_closed` is the dynamic upper bound captured at laptop3's
completion time — record it before running and substitute below
where the runbook says `<L_LAST_CLOSED>`.

## Preconditions checklist

- [ ] Phase 5 repair pass complete on Hetzner (see acceptance log).
- [ ] `backfill_runs` audit table populated with per-worker entries
      (parser SHA + range, per task 0228 AC).
- [ ] BX21 Borg snapshot (or local equivalent) captured before Phase 5
      as a rollback point.
- [ ] No live ingest running. Reads on Phase 5 state must be
      reproducible.
- [ ] `ch-docker` wrapper available (or equivalent `clickhouse-client`
      access into `app-clickhouse-1`).
- [ ] Pre-export-metrics JSON files from each worker available at
      `docs/runbooks/artifacts/{laptop1,laptop2,laptop3}_pre-export-metrics.json`.

## Conventions

- Examples assume `ch-docker` wrapper from
  [`merge-attach-hetzner.sh`](../../scripts/merge-attach-hetzner.sh)
  is on `PATH`. Substitute `clickhouse-client` if running natively.
- All read queries use `FINAL` where the no-FINAL-at-query-time
  invariant doesn't apply (one-shot validation reads, not API hot
  path).
- The runbook fails fast on any unexpected condition — re-run that
  tier after the fix before proceeding.

---

## Tier 1 — Sanity (5–10 min)

### Step 1.1 — Ledger continuity (no gaps)

```sql
SELECT
    min(sequence)                                  AS min_seq,
    max(sequence)                                  AS max_seq,
    count()                                        AS row_count,
    max(sequence) - min(sequence) + 1              AS expected_count,
    (max(sequence) - min(sequence) + 1) - count()  AS gaps
  FROM ledgers
  FORMAT Vertical
```

**Pass criteria**:

- `min_seq = 50_457_424` (task 0228 lower bound)
- `max_seq = <L_LAST_CLOSED>` (captured at laptop3 completion)
- `gaps = 0`

If `gaps > 0`, find them:

```sql
SELECT a.sequence + 1 AS gap_start, b.sequence - 1 AS gap_end
  FROM ledgers AS a
  LEFT JOIN ledgers AS b ON b.sequence = a.sequence + 1
 WHERE b.sequence IS NULL
   AND a.sequence < (SELECT max(sequence) FROM ledgers)
 ORDER BY a.sequence
 LIMIT 50
```

### Step 1.2 — All 19 tables populated

```sql
SELECT table, count() AS parts, sum(rows) AS rows
  FROM system.parts
 WHERE database = 'default' AND active = 1
 GROUP BY table
 ORDER BY table
 FORMAT PrettyCompact
```

**Pass criteria**: all 19 tables present with `rows > 0`. Expected list:

```
accounts                            assets
account_balances_current            ledgers
liquidity_pools                     liquidity_pool_snapshots
lp_positions                        nft_ownership
nft_ownership_pending               nfts
nfts_pending                        operations_appearances
soroban_contracts                   soroban_events
soroban_invocations_appearances     transaction_hash_index
transaction_participants            transactions
wasm_interface_metadata
```

(`nfts` and `nft_ownership` may legitimately have zero rows if the
union range had no Nft-classified contracts. Verify with the audit
trail from `nft_reclassify` — if `promoted_nfts = 0` reported, these
two empty is expected.)

### Step 1.3 — Dictionary loaded

```sql
SELECT name, status, element_count, last_successful_update_time
  FROM system.dictionaries
 WHERE database = 'default'
 FORMAT Vertical
```

**Pass criteria**: `transaction_hash_dict` status = `LOADED`,
`element_count` matches `count() FROM transaction_hash_index`.

### Step 1.4 — No-FINAL invariant for state tables

Per [ADR 0044](../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md),
state-shaped tables must converge under `OPTIMIZE FINAL`. Reads outside
this validation runbook should not need `FINAL`.

Run from a shell:

```bash
for tbl in accounts soroban_contracts assets account_balances_current \
           nfts nfts_pending liquidity_pools lp_positions wasm_interface_metadata; do
  NORMAL=$(ch-docker --query "SELECT count() FROM $tbl")
  FINAL=$(ch-docker --query "SELECT count() FROM $tbl FINAL")
  if [ "$NORMAL" = "$FINAL" ]; then
    echo "OK   $tbl: $NORMAL"
  else
    echo "DIFF $tbl: normal=$NORMAL final=$FINAL"
  fi
done
```

**Pass criteria**: every line `OK`. A `DIFF` line means RMT collapse
incomplete — issue `OPTIMIZE TABLE <tbl> FINAL` and re-check.

---

## Tier 2 — Tier-1 column rebuild correctness (10–20 min)

Each Tier-1 column rebuild must match the fact-table source. Sample
10 random rows per column to spot-check; one diff is a fail.

### Step 2.1 — `accounts.first_seen_ledger`

```sql
SELECT a.account_id,
       a.first_seen_ledger AS stored,
       tp.computed,
       abs(a.first_seen_ledger - tp.computed) AS diff
  FROM (SELECT account_id, first_seen_ledger
          FROM accounts FINAL
         ORDER BY rand() LIMIT 10) a
  LEFT JOIN (SELECT account_id, min(ledger_sequence) AS computed
               FROM transaction_participants
              GROUP BY account_id) tp
    ON tp.account_id = a.account_id
 WHERE diff > 0
 FORMAT PrettyCompact
```

**Pass**: empty result.

### Step 2.2 — `lp_positions.first_deposit_ledger`

```sql
SELECT lp.pool_id, lp.account_id,
       lp.first_deposit_ledger AS stored,
       oa.computed,
       abs(lp.first_deposit_ledger - oa.computed) AS diff
  FROM (SELECT pool_id, account_id, first_deposit_ledger
          FROM lp_positions FINAL
         ORDER BY rand() LIMIT 10) lp
  LEFT JOIN (SELECT pool_id, source_id AS account_id,
                    min(ledger_sequence) AS computed
               FROM operations_appearances
              WHERE type = 22  -- LiquidityPoolDeposit
                AND isNotNull(source_id) AND isNotNull(pool_id)
              GROUP BY pool_id, source_id) oa
    ON oa.pool_id = lp.pool_id AND oa.account_id = lp.account_id
 WHERE diff > 0
 FORMAT PrettyCompact
```

**Pass**: empty result.

### Step 2.3 — `nfts.minted_at_ledger` and `nfts_pending.minted_at_ledger`

```sql
-- nfts
SELECT n.contract_id, n.token_id,
       n.minted_at_ledger AS stored,
       o.computed,
       abs(n.minted_at_ledger - o.computed) AS diff
  FROM (SELECT contract_id, token_id, minted_at_ledger
          FROM nfts FINAL
         ORDER BY rand() LIMIT 10) n
  LEFT JOIN (SELECT contract_id, token_id,
                    min(ledger_sequence) AS computed
               FROM nft_ownership
              WHERE event_type = 0  -- Mint
              GROUP BY contract_id, token_id) o
    ON o.contract_id = n.contract_id AND o.token_id = n.token_id
 WHERE diff > 0
 FORMAT PrettyCompact
```

(Repeat with `nfts_pending` + `nft_ownership_pending`.)

**Pass**: both queries return empty.

### Step 2.4 — `soroban_contracts.deployer_id` + `deployed_at_ledger`

```sql
SELECT sc.contract_id,
       sc.deployer_id        AS stored_deployer,
       sc.deployed_at_ledger AS stored_at,
       d.deployer_id         AS computed_deployer,
       d.deployed_at_ledger  AS computed_at
  FROM (SELECT contract_id, deployer_id, deployed_at_ledger
          FROM soroban_contracts FINAL
         WHERE deployer_id IS NOT NULL
         ORDER BY rand() LIMIT 10) sc
  LEFT JOIN (SELECT contract_id,
                    argMin(deployer_id, wasm_uploaded_at_ledger) AS deployer_id,
                    min(wasm_uploaded_at_ledger) AS deployed_at_ledger
               FROM soroban_contracts
              WHERE isNotNull(deployer_id)
              GROUP BY contract_id) d
    ON d.contract_id = sc.contract_id
 WHERE sc.deployer_id != d.deployer_id
    OR sc.deployed_at_ledger != d.deployed_at_ledger
 FORMAT PrettyCompact
```

**Pass**: empty result.

**Caveat**: this query reads `soroban_contracts` raw on the join side.
After `OPTIMIZE soroban_contracts FINAL` (step 3 of Phase 5), the
raw table holds the post-EXCHANGE rows only — argMin still works
because EXCHANGE made the corrected row canonical. If you ran
`OPTIMIZE soroban_contracts FINAL` _before_ `repair_tier1` by
mistake, this query will appear to pass because both sides see the
same (potentially wrong) data. Cross-check by reading the audit
JSON from the worker's pre-export-metrics — deployer fields should
match the sum of per-worker non-NULL counts.

---

## Tier 3 — Row-count parity vs worker baselines (15–30 min)

Each worker's pre-export-metrics JSON has per-table row counts captured
at FREEZE time. Sum across workers and compare against Hetzner totals.

### Step 3.1 — Build expected totals

```bash
# On a machine that has the artifacts (operator laptop)
jq -s '
  [ .[].tables[] ]
  | group_by(.name)
  | map({
      table: .[0].name,
      expected_total: (map(.rows | tonumber) | add)
    })
' docs/runbooks/artifacts/{laptop1,laptop2,laptop3}_pre-export-metrics.json \
  > /tmp/expected.json
cat /tmp/expected.json
```

### Step 3.2 — Get Hetzner totals

```bash
ssh sorban-prod 'ch-docker --query "
  SELECT name AS table, total_rows
    FROM system.tables
   WHERE database = '\''default'\''
     AND engine LIKE '\''%MergeTree%'\''
   ORDER BY name
   FORMAT JSONEachRow
"' | jq -s '.' > /tmp/hetzner.json
```

### Step 3.3 — Diff

```bash
jq -s --argjson exp "$(cat /tmp/expected.json)" '
  .[0] as $h
  | $exp | map(. as $e
      | { table: .table,
          expected_total,
          hetzner_total: ($h | map(select(.table == $e.table)) | first | .total_rows // 0),
          diff: (($h | map(select(.table == $e.table)) | first | .total_rows // 0)
                  - .expected_total)
        })
' /tmp/hetzner.json | jq '.[] | select(.diff != 0)'
```

**Pass criteria**:

| Table family                                                                                                                                                               | Expected diff                                                                           |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Fact tables (`ledgers`, `transactions`, `transaction_*`, `operations_appearances`, `soroban_*`, `nft_ownership(_pending)`, `liquidity_pool_snapshots`)                     | `diff = 0` (RMT no-version preserves all rows)                                          |
| State tables (`accounts`, `assets`, `account_balances_current`, `nfts`, `nfts_pending`, `lp_positions`, `liquidity_pools`, `soroban_contracts`, `wasm_interface_metadata`) | `diff ≤ 0`, magnitude bounded by RMT collapse (one row per `ORDER BY` key in the union) |

Significant negative diffs on fact tables → STOP, investigate
missing parts. Positive diffs on state tables → STOP, RMT collapse
didn't happen.

---

## Tier 4 — Skeleton, orphans, per-ledger tx (5–10 min)

### Step 4.1 — Skeleton percentage

```sql
SELECT countIf(sequence_number = 0) AS skeletons,
       count()                       AS total,
       round(100 * countIf(sequence_number = 0) / count(), 4) AS pct
  FROM accounts FINAL
  FORMAT Vertical
```

**Pass criteria**: `pct < 1.0` per task 0228 AC §"Skeleton percentage".

### Step 4.2 — Orphan operations (FK to non-existent tx)

```sql
SELECT count() AS orphans
  FROM operations_appearances oa
 WHERE NOT EXISTS (
     SELECT 1 FROM transactions FINAL t WHERE t.id = oa.transaction_id
 )
```

**Pass criteria**: `orphans = 0`.

### Step 4.3 — Per-ledger tx count matches expected

```sql
SELECT l.sequence,
       l.transaction_count       AS expected,
       count(DISTINCT t.id)      AS got,
       l.transaction_count - count(DISTINCT t.id) AS diff
  FROM ledgers l
  LEFT JOIN transactions FINAL t ON t.ledger_sequence = l.sequence
 GROUP BY l.sequence, l.transaction_count
 HAVING diff != 0
 ORDER BY l.sequence
 LIMIT 50
 FORMAT PrettyCompact
```

**Pass criteria**: empty result.

### Step 4.4 — Sequence-number monotonicity per account

```sql
WITH t AS (
  SELECT account_id, ledger_sequence, sequence_number,
         lagInFrame(sequence_number) OVER (
           PARTITION BY account_id ORDER BY ledger_sequence
         ) AS prev_seq
    FROM accounts FINAL
   WHERE sequence_number > 0
)
SELECT count() AS non_monotonic_rows
  FROM t
 WHERE prev_seq IS NOT NULL AND sequence_number < prev_seq
```

**Pass criteria**: `non_monotonic_rows = 0` (Stellar guarantees
sequence_number monotonically increases per account; any drop is a
parser bug or merge anomaly).

---

## Tier 5 — Sample-compare against Horizon (1–2 h)

Per task 0228 acceptance: 1000 stratified ledgers, ≤ 0.01 % mismatch.

### Step 5.1 — Stratified sample

```bash
ssh sorban-prod 'ch-docker --query "
  SELECT sequence
    FROM ledgers
   ORDER BY intDiv(sequence, 500000) ASC, rand() ASC
   LIMIT 100 BY intDiv(sequence, 500000)
   FORMAT TabSeparated
"' > /tmp/sample-ledgers.txt
wc -l /tmp/sample-ledgers.txt  # ~ 100 × (number of partitions)
```

### Step 5.2 — Compare loop

Use the [`compare-with-stellar-api`](../../.claude/skills/compare-with-stellar-api/SKILL.md)
skill for the proper compare (per-tx hash + per-op type/amount, not
just counts):

```bash
# Wrapper that the skill consumes:
while IFS= read -r SEQ; do
  ./scripts/compare-with-stellar-api.sh --ledger "$SEQ" --target sorban-prod
done < /tmp/sample-ledgers.txt 2>&1 | tee /tmp/horizon-compare.log
```

If the skill / wrapper isn't available, a minimal counts-only fallback:

```bash
while IFS= read -r SEQ; do
  HORIZON=$(curl -s "https://horizon.stellar.org/ledgers/$SEQ" \
    | jq -r '"\(.transaction_count)\t\(.operation_count)"')
  CH=$(ssh sorban-prod "ch-docker --query \"
    SELECT (SELECT count() FROM transactions FINAL WHERE ledger_sequence = $SEQ),
           (SELECT count() FROM operations_appearances WHERE ledger_sequence = $SEQ)
    FORMAT TSV\"")
  echo -e "$SEQ\t$HORIZON\t$CH"
done < /tmp/sample-ledgers.txt > /tmp/compare.tsv

# Mismatch ratio
awk -F'\t' '{
  if ($2 != $4 || $3 != $5) mismatch++;
  total++
} END { printf "mismatch %d/%d (%.4f%%)\n", mismatch, total, 100*mismatch/total }' /tmp/compare.tsv
```

**Pass criteria**: `mismatch / total ≤ 0.0001` (0.01 %).

If the mismatch rate is higher, dump the failing ledgers and inspect:

```bash
awk -F'\t' '$2 != $4 || $3 != $5 { print }' /tmp/compare.tsv | head -20
```

Common causes:

- `internal_error` transactions on Horizon counted as
  `transaction_count > 0` but excluded from our parser → expected
  small drift (~0.001 %). Document in the validation report.
- Real data loss → STOP, snapshot rollback, fix root cause, re-do
  Phase 4 + 5.

### Step 5.3 — stellar.expert cross-check

For the 10 highest-mismatch ledgers (if any), fetch the canonical XDR
from stellar.expert / S3 archive and re-parse:

```bash
# Per ledger
aws s3 cp s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/<HEX>--<RANGE>/<file>.xdr.zst /tmp/
zstd -d /tmp/*.zst
# Then run the project's parser to confirm CH state matches XDR.
```

## Tier 6 — Existing repo tooling

Per task 0228 notes:

```bash
ls scripts/diff-merge-vs-truth.sh \
   scripts/gen-merge-snapshots.sh \
   scripts/run-merge-snapshots.sh
```

If still relevant (these were written for the 2/5 fresh-machine
runbook), invoke and capture output.

## Pass / fail aggregation

The runbook passes if every tier passes. Document the result in
`docs/runbooks/artifacts/phase6_validation_$(date -u +%Y%m%d).md` with:

- Per-tier pass/fail
- For each fail: the diagnostic query output + remediation taken
- The compare-against-Horizon mismatch rate (single number)
- Sign-off line: operator + date

This artifact is the input to the go-live decision (per task 0228 AC
"BX21 Borg backup of Hetzner state captured before any read traffic"
follows a passing validation).

## Failure modes & remediation

| Symptom                     | Likely cause                                                 | Fix                                                                                   |
| --------------------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------------- |
| Tier 1.1 `gaps > 0`         | Missing parts from some worker's ATTACH                      | Re-rsync the affected partition from worker; re-ATTACH PART; OPTIMIZE FINAL partition |
| Tier 1.4 `DIFF` lines       | RMT collapse incomplete                                      | `OPTIMIZE TABLE <tbl> FINAL`; re-run Tier 1.4                                         |
| Tier 2 non-empty            | Tier-1 rebuild missed rows                                   | Re-run `backfill-runner repair-tier1` (idempotent); verify staging dir cleanup        |
| Tier 2.4 specifically fails | `OPTIMIZE soroban_contracts FINAL` ran before `repair-tier1` | DATA LOSS scenario — restore from Snapshot A, redo Phase 5 with correct order         |
| Tier 3 fact-table diff < 0  | Missing parts on Hetzner                                     | `system.detached_parts` audit; re-attach                                              |
| Tier 3 state-table diff > 0 | RMT collapse incomplete                                      | `OPTIMIZE TABLE <state_tbl> FINAL`                                                    |
| Tier 4.1 `pct ≥ 1 %`        | `backfill-runner bootstrap` not run, or RPC quota hit        | Re-run bootstrap with Soroban RPC; verify `--soroban-rpc-url` set                     |
| Tier 4.4 non-monotonic > 0  | Parser bug or merge anomaly                                  | Dump offending account; inspect raw XDR; file a parser issue                          |
| Tier 5 mismatch > 0.01 %    | Real data divergence                                         | STOP; analyse sample; consider rollback                                               |

## References

- [ADR 0044 — ClickHouse pilot parallel store](../../lore/2-adrs/0044_clickhouse-pilot-parallel-store.md)
- [ADR 0045 — FREEZE + rsync + ATTACH PART](../../lore/2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md)
- [Task 0228 — parallel-backfill merge & validation](../../lore/1-tasks/active/0228_FEATURE_parallel-backfill-merge-and-validation/README.md)
- [Task 0228 S-approved-plan §"Phase 6 — End-to-end validation"](../../lore/1-tasks/active/0228_FEATURE_parallel-backfill-merge-and-validation/notes/S-approved-plan.md)
- [`compare-with-stellar-api` skill](../../.claude/skills/compare-with-stellar-api/SKILL.md)
- [Companion: 0118 Phase 3 NFT cleanup](0118_phase3_cleanup_nfts.md) — implemented as `backfill-runner nft-reclassify`
- [Companion: 0221 SAC drain from nfts_pending](0221_ch_drain_sac_from_nfts_pending.md)
