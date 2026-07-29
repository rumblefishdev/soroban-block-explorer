# Soroban Block Explorer - 7-Day Post-Launch Monitoring Report (AC6)

Deliverable 3, acceptance criterion 6. Covers the first 7 consecutive days of
public production operation after launch.

- **Launch (Day 1 start):** 2026-07-17T13:40:00Z — the moment the pre-launch
  Basic Auth gate was removed (task 0405)
- **Window end (Day 7 end):** 2026-07-24T13:40:00Z

Each "day" below is a 24-hour period measured from the launch time, **not** a
calendar day. Day 1 therefore starts at 13:40Z on 2026-07-17; the hours before
that are pre-launch and must not be included.

- **Region / account:** eu-central-1 / 750702271865
- **Frontend:** `https://sorobanscan.rumblefish.dev` · **API:** `https://api-sorobanscan.rumblefishdev.com/v1`

## Targets (from D3 acceptance criteria)

| Metric              | Target                  | Source                           |
| ------------------- | ----------------------- | -------------------------------- |
| Uptime              | ≥ 99.9% (aspirational)  | API availability (§ Sources)     |
| API p95 latency     | < 200 ms                | API Gateway `Latency` p95        |
| API error rate      | < 0.1%                  | 5XX / total requests             |
| Ingestion lag       | < 30 s from network tip | CloudWatch `IngestionLagSeconds` |
| Ledger completeness | 0 gaps per day          | ClickHouse gap query             |

## Daily results

| Day | Window (UTC)                        | Uptime % | API p95 (ms) | Error rate % | Max ingest lag (s) | Ledger gaps |
| --- | ----------------------------------- | -------- | ------------ | ------------ | ------------------ | ----------- |
| 1   | 2026-07-17 13:40 → 2026-07-18 13:40 | 100.00   | 330          | 0.000        | 8                  | 0           |
| 2   | 2026-07-18 13:40 → 2026-07-19 13:40 | 100.00   | 142          | 0.000        | 8                  | 0           |
| 3   | 2026-07-19 13:40 → 2026-07-20 13:40 | 100.00   | 553          | 0.000        | 8                  | 0           |
| 4   | 2026-07-20 13:40 → 2026-07-21 13:40 | 100.00   | 496          | 0.000        | 8                  | 0           |
| 5   | 2026-07-21 13:40 → 2026-07-22 13:40 | 100.00   | 149          | 0.000        | 7                  | 0           |
| 6   | 2026-07-22 13:40 → 2026-07-23 13:40 | 100.00   | 162          | 0.000        | 9                  | 0           |
| 7   | 2026-07-23 13:40 → 2026-07-24 13:40 | 100.00   | 165          | 0.000        | 8                  | 0           |

## Summary

- **Uptime (7-day):** 100.00 % derived — target ≥ 99.9%: **PASS**. Derived, not probed: zero 5XX on every day against 30,378 requests actually served, and `production-api-gateway-5xx-rate` OK throughout. Two unrelated alarms did sit raised across the window — both predate it (see Incidents). No Synthetics canary is deployed, so this shows no _observed_ unavailability rather than a measured percentage (§ Sources).
- **p95 (worst day):** 553 ms (Day 3) — target < 200 ms: **FAIL** (expected) — 4 of 7 days were under 200 ms (142/149/162/165); days 1/3/4 exceeded (330/553/496). This is API Gateway `Latency` (server-side, excludes the client network leg), lower than the load test's 577 ms client-side figure. The FAIL is expected for the reasons below.
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
- **Error rate (7-day):** 0.000 % — target < 0.1%: **PASS**. Zero 5XX responses
  out of **30,378 requests** served across the window (per-day counts from
  `AWS/ApiGateway` `Count`: 3,770 · 772 · 784 · 213 · 4,983 · 15,336 · 4,520).
  The zero is a measured value with traffic present on every day, not an absence
  of data.
- **Max ingestion lag:** 9 s (Day 6) — target < 30 s: **PASS** (range 7–9 s across the window)
- **Ledger gaps:** 0 (all 7 days, verified on production ClickHouse) — target 0: **PASS**
- **Incidents / alarms fired:** none during the window — no alarm was raised and none cleared inside it. The API 5XX-rate, Galexie ingestion-lag, ledger-processor error-rate and ClickHouse-write alarms were OK throughout. Two dead-letter-queue depth alarms were already raised when the window opened and stayed raised for its duration. Neither indicates a fault in the launched system, and neither affects the metrics above. The ledger-processor queue does not bear on completeness — that is independently verified at 0 gaps on every day. The enrichment queue holds fetches of off-chain metadata that is permanently unreachable at its source; the chain data itself is complete.

  - `production-enrichment-dlq-depth` — raised 2026-07-03, metadata-enrichment fetches that permanently fail against unreachable external sources.
  - `production-ledger-processor-dlq-depth` — raised 2026-07-09T19:13Z, eight days pre-launch, holding ~7,950 queued S3 notifications. These are wake-up signals rather than data: the indexer ignores the message body and reconciles from ClickHouse's durable `max(sequence)` cursor on every invocation, so a dropped signal cannot drop a ledger (`crates/indexer/src/handler/mod.rs`). The queue emptied on 2026-07-24 when the messages reached their 14-day retention limit, and the alarm returned to OK at 14:50Z, after the window had closed.

  Both alarms fire on queue depth > 0, so a non-empty queue keeps them raised with no ongoing failure. Neither affected the tracked metrics: ledger completeness is independently verified at **0 gaps on every one of the seven days** (§ Sources), API error rate was 0.000 %, and ingestion lag stayed at 7–9 s.

## Sources — how each column is produced

Every figure in the table above is reproducible from the sources below. All
CloudWatch reads use `--region eu-central-1` and a per-day period of 86400 s over
the seven 24-hour buckets measured from the launch timestamp.

### CloudWatch (uptime, p95, error rate)

- **API p95:** namespace `AWS/ApiGateway`, metric `Latency`, ExtendedStatistic
  `p95`, dimension `ApiName=production-soroban-explorer-api` (REST API id
  `6l9k06w4pl`, stage `production`).
- **Error rate:** `AWS/ApiGateway` `5XXError` (Sum) ÷ `Count` (Sum) per day. The
  Lambda-side cross-check is `AWS/Lambda` `Errors` ÷ `Invocations` with
  `FunctionName=production-soroban-explorer-api`.
- **Uptime:** no CloudWatch Synthetics canary is deployed, so uptime is **derived,
  not externally probed** — the authoritative signals are (a) `5XXError` Sum per
  day and (b) the state history of the availability alarm
  `production-api-gateway-5xx-rate` (`describe-alarm-history`). Both were clean for
  every day of the window: zero 5XX responses and no alarm transition, so no
  period of unavailability was observed. This method cannot resolve an outage
  shorter than the alarm's evaluation period, and it measures the API rather than
  the frontend.

### ClickHouse (ledger completeness; lag via CloudWatch)

Run on prod ClickHouse (`app-clickhouse-1`, creds via read-rsp → env). The
`ledgers` table is `(sequence, hash, closed_at, protocol_version,
transaction_count, base_fee)` — `closed_at` is the ledger close time, and there
is **no ingest/write-time column**.

Day buckets are 24 h from the launch time, so they line up with the table above
rather than with calendar dates:

```sql
-- Ledger gaps per day: expected span − distinct sequences (0 = no gaps)
SELECT intDiv(dateDiff('second',
               toDateTime('2026-07-17 13:40:00', 'UTC'), closed_at), 86400) + 1 AS day,
       (max(sequence) - min(sequence) + 1) - count(DISTINCT sequence) AS gaps
FROM ledgers
WHERE closed_at >= toDateTime('2026-07-17 13:40:00', 'UTC')
  AND closed_at <  toDateTime('2026-07-24 13:40:00', 'UTC')
GROUP BY day ORDER BY day;
```

Also run it without the `GROUP BY` to check the window as a whole — a gap that
falls exactly on a day boundary is invisible to the per-day form, because each
day's `min`/`max` are taken inside that day:

```sql
SELECT (max(sequence) - min(sequence) + 1) - count(DISTINCT sequence) AS gaps_total
FROM ledgers
WHERE closed_at >= toDateTime('2026-07-17 13:40:00', 'UTC')
  AND closed_at <  toDateTime('2026-07-24 13:40:00', 'UTC');
```

**Ingestion lag comes from CloudWatch, not ClickHouse.** No `ledgers` row carries
its write time, so close-to-ingest cannot be reconstructed after the fact from
the database. Instead the indexer emits it at write time:

- **Metric:** `SorobanBlockExplorer/Indexer` / **`IngestionLagSeconds`**
  (unit `Seconds`, dimension `Environment=production`) — wall-clock seconds
  between ledger close and the row being committed. Emitted per ledger from
  `publish_indexer_metrics` (`crates/indexer/src/handler/mod.rs`, task 0399);
  **live in production since 2026-07-17**, so it accumulates from day 1 of the
  window.
- **Per-day read:** `get-metric-statistics` with `--statistics Average Maximum`
  and `--period 86400`, exactly like the API Gateway columns below. Use
  `Maximum` for the "max ingest lag" column and `Average` for context.

Reference sample (75 minutes on 2026-07-17, 15 × 5-minute datapoints): average
3.1 s, worst 6.0 s, against the < 30 s criterion.

### Generator skeleton (optional)

The report is produced once, so a full pipeline is overkill. This skeleton pulls
the CloudWatch columns for the 7 days; paste the ClickHouse gap numbers in by
hand. Only `API_NAME` still needs filling.

```bash
#!/usr/bin/env bash
# ponytail: one-shot report helper; fill API_NAME, emits partial md rows.
# Self-check: dry-run one day and eyeball p95 vs the CloudWatch console.
set -euo pipefail
API_NAME="production-soroban-explorer-api"   # REST API id 6l9k06w4pl, stage production
REGION="eu-central-1"; PROFILE="sorobanscan"
LAUNCH="2026-07-17T13:40:00Z"   # buckets are 24h from launch, NOT midnight
for d in 0 1 2 3 4 5 6; do
  # macos date; -f must match LAUNCH's format or the offsets silently go wrong
  start=$(date -u -j -v+${d}d -f %Y-%m-%dT%H:%M:%SZ "$LAUNCH" +%Y-%m-%dT%H:%M:%SZ)
  end=$(date -u -j -v+$((d+1))d -f %Y-%m-%dT%H:%M:%SZ "$LAUNCH" +%Y-%m-%dT%H:%M:%SZ)
  p95=$(aws cloudwatch get-metric-statistics --region "$REGION" --profile "$PROFILE" \
    --namespace AWS/ApiGateway --metric-name Latency \
    --dimensions Name=ApiName,Value="$API_NAME" \
    --start-time "$start" --end-time "$end" --period 86400 \
    --extended-statistics p95 --query 'Datapoints[0].ExtendedStatistics.p95' --output text)
  err5xx=$(aws cloudwatch get-metric-statistics --region "$REGION" --profile "$PROFILE" \
    --namespace AWS/ApiGateway --metric-name 5XXError \
    --dimensions Name=ApiName,Value="$API_NAME" \
    --start-time "$start" --end-time "$end" --period 86400 \
    --statistics Sum --query 'Datapoints[0].Sum' --output text)
  total=$(aws cloudwatch get-metric-statistics --region "$REGION" --profile "$PROFILE" \
    --namespace AWS/ApiGateway --metric-name Count \
    --dimensions Name=ApiName,Value="$API_NAME" \
    --start-time "$start" --end-time "$end" --period 86400 \
    --statistics Sum --query 'Datapoints[0].Sum' --output text)
  lag=$(aws cloudwatch get-metric-statistics --region "$REGION" --profile "$PROFILE" \
    --namespace SorobanBlockExplorer/Indexer --metric-name IngestionLagSeconds \
    --dimensions Name=Environment,Value=production \
    --start-time "$start" --end-time "$end" --period 86400 \
    --statistics Maximum --query 'Datapoints[0].Maximum' --output text)
  echo "| $((d+1)) | ${start} → ${end} | <uptime> | ${p95:-NA} | 5xx=${err5xx:-0}/${total:-0} | ${lag:-NA} | <gaps from CH> |"
done
```

## Notes

- Ingestion lag has no historical ClickHouse source (the `ledgers` table stores
  no write time), so the per-day figures come from the CloudWatch
  `IngestionLagSeconds` metric emitted by the indexer (task 0399, live since
  2026-07-17). It cannot be backfilled for any period before that date.
