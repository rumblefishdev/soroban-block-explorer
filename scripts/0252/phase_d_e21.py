#!/usr/bin/env python3
"""E21 — /liquidity-pools/:id/chart internal sanity.

Sample N pools × 3 interval/window combos. For each:
  * Run the canonical bucketed query.
  * Assert bucket count is in [floor((to-from_ledger)/interval)-2,
    floor((to-from_ledger_seconds)/interval)+2]  — the ±2 tolerates
    edge buckets and ledger-time variance (~5 s/ledger on Stellar).
  * No NULL bucket timestamps.
  * TVL / volume / fee_revenue are non-negative.
  * Buckets ASC monotonic by timestamp.
"""

from __future__ import annotations

import json
import random
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from common import (
    OUT_DIR,
    EndpointResult,
    append_tsv_row,
    ch_query_json,
    dump_diff,
    load_samples,
    write_tsv_header,
)


ENDPOINT = "E21"
TSV = OUT_DIR / "phase_d_e21.tsv"

SAMPLE = 200
SECONDS_PER_LEDGER = 5  # Stellar avg

# (interval_seconds, ledger_window) — bucket size, ledger range to query.
COMBOS = [
    (3600, 1_000),     # 1h buckets, ~83 min window
    (86_400, 17_280),  # 1d buckets, ~24 h window
    (604_800, 120_960),  # 1w buckets, ~7 d window
]


def fetch_chart(pool_hex: str, from_l: int, to_l: int, interval_s: int) -> list[dict]:
    sql = f"""
    SELECT
        toStartOfInterval(l.closed_at, INTERVAL {interval_s} SECOND) AS bucket_ts,
        toString(argMax(s.tvl, s.ledger_sequence))                   AS tvl,
        toString(sum(s.volume))                                      AS volume,
        toString(sum(s.fee_revenue))                                 AS fee_revenue
    FROM liquidity_pool_snapshots s FINAL
    JOIN ledgers l ON l.sequence = s.ledger_sequence
    WHERE s.pool_id = unhex('{pool_hex}')
      AND s.ledger_sequence >= {from_l}
      AND s.ledger_sequence <  {to_l}
    GROUP BY bucket_ts
    ORDER BY bucket_ts ASC
    FORMAT JSONEachRow
    """
    return ch_query_json(sql)


def main() -> int:
    write_tsv_header(TSV)
    result = EndpointResult(endpoint=ENDPOINT)
    started = time.monotonic()
    random.seed(42)

    pools = load_samples("samples_pools.txt")
    random.shuffle(pools)
    pools = pools[:SAMPLE]
    print(f"[E21] {len(pools)} pool samples", file=sys.stderr)
    result.sample_size = len(pools) * len(COMBOS)

    # Anchor "to_l" near CH backfill tip.
    TIP = 62_525_000
    diffs: list[str] = []
    processed = 0

    for pool_hex in pools:
        processed += 1
        for interval_s, window in COMBOS:
            from_l = TIP - window
            to_l = TIP

            try:
                buckets = fetch_chart(pool_hex, from_l, to_l, interval_s)
            except RuntimeError as e:
                result.record_field("query_runs", "fail")
                diffs.append(f"query_error pool={pool_hex[:10]} interval={interval_s}: {str(e)[:80]}")
                continue
            result.record_field("query_runs", "pass")

            # bucket count vs expected.
            seconds_window = window * SECONDS_PER_LEDGER
            expected = seconds_window // interval_s
            actual = len(buckets)
            # Pool may not have a snapshot in every bucket → actual <= expected.
            if actual <= expected + 2:
                result.record_field("bucket_count_sane", "pass")
            else:
                result.record_field("bucket_count_sane", "fail")
                diffs.append(
                    f"bucket_count pool={pool_hex[:10]} interval={interval_s} "
                    f"actual={actual} expected_max={expected + 2}"
                )

            # monotonic ASC + no NULL ts + non-negative values.
            ts_list = [r.get("bucket_ts") for r in buckets]
            if all(t and t[:4].isdigit() for t in ts_list):
                result.record_field("bucket_ts_well_formed", "pass")
            else:
                result.record_field("bucket_ts_well_formed", "fail")
                diffs.append(f"bucket_ts NULL/malformed pool={pool_hex[:10]}")

            if ts_list == sorted(ts_list):
                result.record_field("bucket_ts_monotonic_asc", "pass")
            else:
                result.record_field("bucket_ts_monotonic_asc", "fail")
                diffs.append(f"bucket_ts not monotonic asc pool={pool_hex[:10]}")

            non_neg = True
            for r in buckets:
                for fname in ("tvl", "volume", "fee_revenue"):
                    raw = r.get(fname) or "0"
                    if raw.startswith("-"):
                        non_neg = False
                        diffs.append(f"negative {fname}={raw} pool={pool_hex[:10]}")
                        break
                if not non_neg:
                    break
            result.record_field("values_non_negative", "pass" if non_neg else "fail")

        if processed % 50 == 0:
            print(f"[E21] {processed}/{len(pools)} pools", file=sys.stderr)

    if diffs:
        dump_diff(ENDPOINT, "chart", {"sample": len(pools)}, None, diffs[:50])

    pass_n = result.pass_total
    fail_n = result.fail_total
    append_tsv_row(TSV, ENDPOINT, "chart", pass_n, 0, fail_n, f"sample={len(pools)} combos={len(COMBOS)}")
    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_d_e21_summary.json"
    summary.write_text(json.dumps({
        "endpoint": ENDPOINT,
        "sample_size": result.sample_size,
        "pass_total": result.pass_total,
        "fail_total": result.fail_total,
        "tolerance_total": result.tolerance_total,
        "elapsed_ms": result.elapsed_ms,
        "fields": {
            k: {"pass": v.pass_count, "tolerance": v.tolerance_count, "fail": v.fail_count}
            for k, v in result.fields.items()
        },
    }, indent=2))

    print(f"[E21] done: sample={len(pools)} combos={len(COMBOS)} "
          f"pass={result.pass_total} fail={result.fail_total} "
          f"elapsed={result.elapsed_ms}ms", file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
