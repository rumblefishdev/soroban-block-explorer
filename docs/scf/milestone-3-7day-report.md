# Soroban Block Explorer - 7-Day Post-Launch Monitoring Report (AC6)

Deliverable 3, acceptance criterion 6. Covers the first 7 consecutive days of
public production operation after launch.

- **Launch (Day 1 start):** <TODO: YYYY-MM-DD HH:MMZ>
- **Window end (Day 7 end):** <TODO: YYYY-MM-DD HH:MMZ>
- **Region / account:** eu-central-1 / 750702271865
- **Frontend:** `https://sorobanscan.rumblefish.dev` · **API:** `https://api-sorobanscan.rumblefishdev.com/v1`

## Targets (from D3 acceptance criteria)

| Metric              | Target                  | Source                       |
| ------------------- | ----------------------- | ---------------------------- |
| Uptime              | ≥ 99.9% (aspirational)  | API availability (§ Sources) |
| API p95 latency     | < 200 ms                | API Gateway `Latency` p95    |
| API error rate      | < 0.1%                  | 5XX / total requests         |
| Ingestion lag       | < 30 s from network tip | Live sample / CloudWatch (§) |
| Ledger completeness | 0 gaps per day          | ClickHouse gap query         |

## Daily results

| Day | Date   | Uptime % | API p95 (ms) | Error rate % | Max ingest lag (s) | Ledger gaps |
| --- | ------ | -------- | ------------ | ------------ | ------------------ | ----------- |
| 1   | <TODO> | <TODO>   | <TODO>       | <TODO>       | <TODO>             | <TODO>      |
| 2   | <TODO> | <TODO>   | <TODO>       | <TODO>       | <TODO>             | <TODO>      |
| 3   | <TODO> | <TODO>   | <TODO>       | <TODO>       | <TODO>             | <TODO>      |
| 4   | <TODO> | <TODO>   | <TODO>       | <TODO>       | <TODO>             | <TODO>      |
| 5   | <TODO> | <TODO>   | <TODO>       | <TODO>       | <TODO>             | <TODO>      |
| 6   | <TODO> | <TODO>   | <TODO>       | <TODO>       | <TODO>             | <TODO>      |
| 7   | <TODO> | <TODO>   | <TODO>       | <TODO>       | <TODO>             | <TODO>      |

## Summary

- **Uptime (7-day):** <TODO> — target ≥ 99.9%: <PASS/FAIL>
- **p95 (worst day):** <TODO> ms — target < 200 ms: <PASS/FAIL>
  > **Expect FAIL, and do not present it as a surprise.** The 2026-07-17 load
  > test measured p95 = 577 ms at the required load (p50 = 168 ms, errors
  > 0.000%). Unless the AC4 follow-ups have shipped by the time this window
  > closes, this line will read FAIL for the same three reasons documented in
  > `milestone-3-evidence.md` § AC4 — cross-region archive fetch (`txdetail`),
  > third-party IPFS fetch (`nftdetail`), and `lplist`'s creation-ledger scan.
  > Cross-reference that section rather than re-explaining it here. Note this
  > report measures API Gateway `Latency`, which **excludes** the client-side
  > network leg the load test includes — the two numbers are not directly
  > comparable; say which one you are quoting.
- **Error rate (7-day):** <TODO> % — target < 0.1%: <PASS/FAIL>
- **Max ingestion lag:** <TODO> s — target < 30 s: <PASS/FAIL>
- **Ledger gaps:** <TODO> — target 0: <PASS/FAIL>
- **Incidents / alarms fired:** <TODO: list, or "none">

## Sources — how each column is produced

Fill the table from these. Metric dimension names marked `<TODO>` must be
confirmed against the deployed CloudWatch resources (dashboard JSON or
`aws cloudwatch list-metrics`) before the first run.

### CloudWatch (uptime, p95, error rate)

- **API p95:** namespace `AWS/ApiGateway`, metric `Latency`, ExtendedStatistic
  `p95`, dimension `ApiName=<TODO>` (+ `Stage=<TODO>` if staged). Per-day period.
- **Error rate:** `AWS/ApiGateway` `5XXError` (Sum) ÷ `Count` (Sum) per day.
  Cross-check Lambda-side with `AWS/Lambda` `Errors` ÷ `Invocations`,
  `FunctionName=<TODO: production API fn>`.
- **Uptime:** 1 − (minutes with error-rate breach or health-check failure) ÷
  total minutes. <TODO: pick the authoritative source — a CloudWatch Synthetics
  canary against the frontend/API, or derive from 5XX minutes. Note the choice.>

### ClickHouse (ledger completeness; lag via live sample)

Run on prod ClickHouse (`app-clickhouse-1`, creds via read-rsp → env). The
`ledgers` table is `(sequence, hash, closed_at, protocol_version,
transaction_count, base_fee)` — `closed_at` is the ledger close time, and there
is **no ingest/write-time column**.

```sql
-- Ledger gaps per day: expected span − distinct sequences (0 = no gaps)
SELECT toDate(closed_at) AS day,
       (max(sequence) - min(sequence) + 1) - count(DISTINCT sequence) AS gaps
FROM ledgers
WHERE closed_at BETWEEN '<LAUNCH>' AND '<END>'
GROUP BY day ORDER BY day;
```

**Ingestion lag has no historical ClickHouse source** — no row carries its write
time, so close-to-ingest cannot be computed after the fact. Get it one of two
ways, decided before the window opens:

- **Live sample:** scrape `SELECT now() - max(closed_at) FROM ledgers` on a
  schedule (how far the newest ingested ledger trails wall-clock) and take the
  daily max. This is the same seconds-lag signal AC3 needs.
- **CloudWatch metric (recommended):** the indexer already publishes a custom
  metric — `SorobanBlockExplorer/Indexer / LastProcessedLedgerSequence`, in
  `publish_ledger_sequence_metric` (`crates/indexer/src/handler/mod.rs`), with
  `cloudwatch:PutMetricData` already granted. Add a second datum
  `IngestionLagSeconds = now − ledger.closed_at` in the same spot (~5 lines,
  indexer-only — no API-types regen; ships via `make deploy-production-compute`)
  and read it per-day like the API metrics. This also closes the AC3 seconds-lag
  gap.

Today CloudWatch has throughput / sequence / queue-depth / Lambda-duration
signals, but **no seconds-based end-to-end lag** — so one of the two above must
be wired.

<TODO: emit IngestionLagSeconds before launch so it accumulates from day 1 (AC3 / task 0129).>

### Generator skeleton (optional)

The report is produced once, so a full pipeline is overkill. This skeleton pulls
the CloudWatch columns for the 7 days; paste the ClickHouse results in by hand.
Confirm every `<TODO>` dimension first.

```bash
#!/usr/bin/env bash
# ponytail: one-shot report helper; fill <TODO> dims, emits partial md rows.
# Self-check: dry-run one day and eyeball p95 vs the CloudWatch console.
set -euo pipefail
API_NAME="<TODO>"; REGION="eu-central-1"; LAUNCH="<TODO: YYYY-MM-DD>"
for d in 0 1 2 3 4 5 6; do
  start=$(date -u -j -v+${d}d -f %Y-%m-%d "$LAUNCH" +%Y-%m-%dT00:00:00Z)   # macos date
  end=$(date -u -j -v+$((d+1))d -f %Y-%m-%d "$LAUNCH" +%Y-%m-%dT00:00:00Z)
  p95=$(aws cloudwatch get-metric-statistics --region "$REGION" \
    --namespace AWS/ApiGateway --metric-name Latency \
    --dimensions Name=ApiName,Value="$API_NAME" \
    --start-time "$start" --end-time "$end" --period 86400 \
    --extended-statistics p95 --query 'Datapoints[0].ExtendedStatistics.p95' --output text)
  err5xx=$(aws cloudwatch get-metric-statistics --region "$REGION" \
    --namespace AWS/ApiGateway --metric-name 5XXError \
    --dimensions Name=ApiName,Value="$API_NAME" \
    --start-time "$start" --end-time "$end" --period 86400 \
    --statistics Sum --query 'Datapoints[0].Sum' --output text)
  total=$(aws cloudwatch get-metric-statistics --region "$REGION" \
    --namespace AWS/ApiGateway --metric-name Count \
    --dimensions Name=ApiName,Value="$API_NAME" \
    --start-time "$start" --end-time "$end" --period 86400 \
    --statistics Sum --query 'Datapoints[0].Sum' --output text)
  echo "| $((d+1)) | ${start%T*} | <lag/gaps from CH> | ${p95:-NA} | 5xx=${err5xx:-0}/${total:-0} |"
done
```

## Notes

- Ingestion lag has no historical ClickHouse source (the `ledgers` table stores
  no write time). The per-day lag must come from a live sample or a CloudWatch
  metric wired before the window — the same seconds-lag gap AC3 tracks (task
  0129).
- Deliver the finished report to the Stellar team (task 0127 AC).
