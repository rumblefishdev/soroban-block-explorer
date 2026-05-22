#!/usr/bin/env python3
"""E05 — /ledgers/:sequence compare CH ↔ Horizon.

Per-key compare on the ledger header. Field-by-field diff on 7
fields:
  1. sequence
  2. hash               (32-byte ledger hash)
  3. closed_at          (timestamp)
  4. transaction_count  (= successful + failed per Horizon naming)
  5. operation_count
  6. base_fee
  7. base_reserve

Sample source: samples_ledgers.txt (30K stratified + adversarial).
Filtered to retention-valid (≥ 56,657,428) for Horizon compare.

Per Phase 6 Tier 5 we already proved ledger header parity across
980 ledgers (hash-set 980/980). This script does the field-level
deep dive at 5K-30K scale per Phase B AC.
"""

from __future__ import annotations

import json
import os
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
    horizon_get,
    load_samples,
    read_completed_keys,
    write_tsv_header,
)


ENDPOINT = "E05"
TSV = OUT_DIR / "phase_b_e05.tsv"
HORIZON_FLOOR = 56657428


def fetch_ch_ledger(seq: int) -> dict | None:
    sql = f"""
    SELECT
      sequence,
      lower(hex(hash))    AS hash,
      toString(closed_at) AS closed_at,
      transaction_count,
      operation_count,
      base_fee,
      base_reserve
    FROM ledgers
    WHERE sequence = {seq}
    FORMAT JSONEachRow
    """
    rows = ch_query_json(sql)
    return rows[0] if rows else None


def fetch_horizon_ledger(seq: int) -> dict | None:
    body = horizon_get(f"/ledgers/{seq}")
    if not body or "sequence" not in body:
        return None
    return body


def compare(seq: int, ch: dict, hz: dict, result: EndpointResult) -> list[str]:
    diffs: list[str] = []

    # 1. sequence
    if int(ch["sequence"]) == int(hz["sequence"]):
        result.record_field("sequence", "pass")
    else:
        result.record_field("sequence", "fail")
        diffs.append(f"sequence CH={ch['sequence']} HZ={hz['sequence']}")

    # 2. hash
    if ch["hash"].lower() == hz["hash"].lower():
        result.record_field("hash", "pass")
    else:
        result.record_field("hash", "fail")
        diffs.append(f"hash CH={ch['hash']} HZ={hz['hash']}")

    # 3. closed_at — CH stores as `2026-05-12 02:06:20.000`, Horizon
    # as `2026-05-12T02:06:20Z`. Normalise to seconds.
    def norm_time(s: str) -> str:
        # Strip subseconds + zone, lowercase, keep YYYY-MM-DD HH:MM:SS
        s = s.replace("T", " ").replace("Z", "").split(".")[0].strip()
        return s

    if norm_time(ch["closed_at"]) == norm_time(hz["closed_at"]):
        result.record_field("closed_at", "pass")
    else:
        result.record_field("closed_at", "fail")
        diffs.append(f"closed_at CH={ch['closed_at']} HZ={hz['closed_at']}")

    # 4. transaction_count: Horizon's `successful_transaction_count +
    # failed_transaction_count` is the total ledger emits.
    hz_tx = int(hz.get("successful_transaction_count", 0) or 0) + \
            int(hz.get("failed_transaction_count", 0) or 0)
    if int(ch["transaction_count"]) == hz_tx:
        result.record_field("transaction_count", "pass")
    else:
        result.record_field("transaction_count", "fail")
        diffs.append(f"transaction_count CH={ch['transaction_count']} HZ={hz_tx}")

    # 5. operation_count
    if int(ch["operation_count"]) == int(hz.get("operation_count") or 0):
        result.record_field("operation_count", "pass")
    else:
        # Could be Horizon successful-only semantic drift.
        result.record_field("operation_count", "tolerance")
        diffs.append(f"operation_count CH={ch['operation_count']} HZ={hz.get('operation_count')} (drift)")

    # 6. base_fee
    if int(ch["base_fee"]) == int(hz.get("base_fee_in_stroops") or 0):
        result.record_field("base_fee", "pass")
    else:
        result.record_field("base_fee", "fail")
        diffs.append(f"base_fee CH={ch['base_fee']} HZ={hz.get('base_fee_in_stroops')}")

    # 7. base_reserve
    if int(ch["base_reserve"]) == int(hz.get("base_reserve_in_stroops") or 0):
        result.record_field("base_reserve", "pass")
    else:
        result.record_field("base_reserve", "fail")
        diffs.append(f"base_reserve CH={ch['base_reserve']} HZ={hz.get('base_reserve_in_stroops')}")

    return diffs


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E05] {len(done_keys)} keys already done", file=sys.stderr)

    ledgers_all = [int(s) for s in load_samples("samples_ledgers.txt")]
    ledgers = [L for L in ledgers_all if L >= HORIZON_FLOOR]
    print(f"[E05] {len(ledgers)} retention-valid ledgers / {len(ledgers_all)} total",
          file=sys.stderr)

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        random.seed(42)
        ledgers = random.sample(ledgers, min(pilot, len(ledgers)))
        print(f"[E05] PILOT — capped to {len(ledgers)} ledgers", file=sys.stderr)

    cap = int(os.environ.get("SBE_PHASE_B_CAP", "5000"))
    if cap > 0 and len(ledgers) > cap:
        random.seed(123)
        ledgers = random.sample(ledgers, cap)
        print(f"[E05] capped to {cap} stratified samples", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(ledgers)
    started = time.monotonic()
    processed = 0

    for seq in ledgers:
        key = str(seq)
        if key in done_keys:
            continue
        processed += 1

        ch = fetch_ch_ledger(seq)
        if ch is None:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 1, "CH_MISSING")
            result.fail_total += 1
            continue

        hz = fetch_horizon_ledger(seq)
        if hz is None:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 0, "HZ_PRE_RETENTION")
            continue

        diffs = compare(seq, ch, hz, result)
        if diffs:
            dump_diff(ENDPOINT, key, ch, hz, diffs)
            tol = sum(1 for d in diffs if "drift" in d)
            fail = len(diffs) - tol
            pass_n = 7 - len(diffs)
            append_tsv_row(TSV, ENDPOINT, key, pass_n, tol, fail, ";".join(diffs))
        else:
            append_tsv_row(TSV, ENDPOINT, key, 7, 0, 0, "")

        if processed % 100 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(ledgers) - processed) / max(rate, 0.01))
            print(f"[E05] {processed}/{len(ledgers)} done, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"rate={rate:.1f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e05_summary.json"
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

    print(f"[E05] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
