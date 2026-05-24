#!/usr/bin/env python3
"""E04 — /ledgers list compare CH ↔ Horizon.

Per-ledger detail compare. The list endpoint orders by
`(closed_at DESC, sequence DESC)` and projects a small set of fields
straight off the `ledgers` table. Sequence-vs-closed_at monotonicity
on Stellar makes ordering trivially correct; the value-add of the
compare is field correctness on the per-ledger projection.

Per anchor (= per ledger):
  1. Pick `anchor_ledger` from samples_ledgers.txt (retention-valid).
  2. CH: SELECT one row from `ledgers` WHERE sequence = anchor_ledger.
  3. Horizon: GET /ledgers/:sequence.
  4. Field compare:
     - hash             (32-byte hex)        strict
     - closed_at        (DateTime)           strict (Horizon ISO8601 vs CH naive UTC normalised)
     - protocol_version (Int)                strict
     - transaction_count (Int)               tolerance (CH = total; Horizon ssplits successful + failed)
     - base_fee_in_stroops (Int)             strict

Sample: 600 anchors. No within-ledger slicing → 1 row per anchor, so
the run is bounded by Horizon RPS not page count. ETA ~5 min.
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


ENDPOINT = "E04"
TSV = OUT_DIR / "phase_b_e04.tsv"

HORIZON_FLOOR = 56657428
CH_TIP_FLOOR = 62525000

DEFAULT_ANCHORS = int(os.environ.get("SBE_E04_ANCHORS", "600"))


def fetch_ch_ledger(seq: int) -> dict | None:
    sql = f"""
    SELECT
        sequence                          AS sequence,
        lower(hex(hash))                  AS hash,
        toString(closed_at)               AS closed_at,
        protocol_version                  AS protocol_version,
        transaction_count                 AS transaction_count,
        base_fee                          AS base_fee
    FROM ledgers
    WHERE sequence = {seq}
    LIMIT 1
    FORMAT JSONEachRow
    """
    rows = ch_query_json(sql)
    return rows[0] if rows else None


def fetch_horizon_ledger(seq: int) -> dict | None:
    body = horizon_get(f"/ledgers/{seq}")
    if not body or "sequence" not in body:
        return None
    return body


def normalise_closed_at(s: str) -> str:
    """CH `toString(DateTime)` = `2024-01-15 12:34:56`.
    Horizon `closed_at`     = `2024-01-15T12:34:56Z`.
    Strip non-digits → 14-digit YYYYMMDDhhmmss for raw compare.
    """
    return "".join(c for c in s if c.isdigit())[:14]


def compare_ledger(seq: int, ch: dict, hz: dict, result: EndpointResult) -> list[str]:
    diffs: list[str] = []

    # hash
    ch_hash = ch["hash"].lower()
    hz_hash = (hz.get("hash") or "").lower()
    if ch_hash == hz_hash:
        result.record_field("hash", "pass")
    else:
        result.record_field("hash", "fail")
        diffs.append(f"hash CH={ch_hash} HZ={hz_hash}")

    # closed_at
    ch_ts = normalise_closed_at(ch["closed_at"])
    hz_ts = normalise_closed_at(hz.get("closed_at", ""))
    if ch_ts == hz_ts:
        result.record_field("closed_at", "pass")
    else:
        result.record_field("closed_at", "fail")
        diffs.append(f"closed_at CH={ch['closed_at']} HZ={hz.get('closed_at')}")

    # protocol_version
    if int(ch["protocol_version"]) == int(hz.get("protocol_version", -1)):
        result.record_field("protocol_version", "pass")
    else:
        result.record_field("protocol_version", "fail")
        diffs.append(f"protocol_version CH={ch['protocol_version']} HZ={hz.get('protocol_version')}")

    # transaction_count — tolerance: Horizon splits successful + failed; CH
    # holds total. Compare CH vs successful+failed; if equal pass, otherwise
    # tolerance (Horizon semantic).
    ch_tx = int(ch["transaction_count"])
    hz_succ = int(hz.get("successful_transaction_count") or 0)
    hz_fail = int(hz.get("failed_transaction_count") or 0)
    hz_total = hz_succ + hz_fail
    if ch_tx == hz_total:
        result.record_field("transaction_count", "pass")
    elif ch_tx == hz_succ:
        result.record_field("transaction_count", "tolerance")
        diffs.append(f"transaction_count CH={ch_tx} HZ_success={hz_succ} HZ_failed={hz_fail} "
                     f"(CH=success-only ingestion)")
    else:
        result.record_field("transaction_count", "fail")
        diffs.append(f"transaction_count CH={ch_tx} HZ_total={hz_total} "
                     f"(success={hz_succ} failed={hz_fail})")

    # base_fee
    ch_fee = int(ch["base_fee"])
    hz_fee = int(hz.get("base_fee_in_stroops") or 0)
    if ch_fee == hz_fee:
        result.record_field("base_fee", "pass")
    else:
        result.record_field("base_fee", "fail")
        diffs.append(f"base_fee CH={ch_fee} HZ={hz_fee}")

    return diffs


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E04] {len(done_keys)} ledgers already done", file=sys.stderr)

    all_ledgers = [int(s) for s in load_samples("samples_ledgers.txt")]
    print(f"[E04] {len(all_ledgers)} ledger samples loaded", file=sys.stderr)

    retention_safe = [L for L in all_ledgers
                      if HORIZON_FLOOR <= L <= CH_TIP_FLOOR]
    random.seed(42)
    anchors = random.sample(retention_safe, min(DEFAULT_ANCHORS, len(retention_safe)))
    print(f"[E04] {len(anchors)} anchors picked", file=sys.stderr)

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        anchors = anchors[:pilot]
        print(f"[E04] PILOT mode — first {pilot} anchors", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(anchors)
    started = time.monotonic()
    processed = 0

    for seq in anchors:
        key = str(seq)
        if key in done_keys:
            continue
        processed += 1

        try:
            ch = fetch_ch_ledger(seq)
        except RuntimeError as e:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 1, f"CH_ERROR:{str(e)[:120]}")
            result.fail_total += 1
            continue

        if ch is None:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 1, "CH_MISSING")
            result.fail_total += 1
            continue

        hz = fetch_horizon_ledger(seq)
        if hz is None:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 0, "HZ_PRE_RETENTION")
            continue

        diffs = compare_ledger(seq, ch, hz, result)

        pass_n = 5 - len(diffs)
        tol_n = sum(1 for d in diffs if "(CH=success-only" in d)
        fail_n = len(diffs) - tol_n

        if diffs:
            dump_diff(ENDPOINT, key, ch, hz, diffs)
            note = ";".join(diffs)[:500]
        else:
            note = ""

        append_tsv_row(TSV, ENDPOINT, key, pass_n, tol_n, fail_n, note)

        if processed % 50 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(anchors) - processed) / max(rate, 0.01))
            print(f"[E04] {processed}/{len(anchors)} done, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"tol={result.tolerance_total} rate={rate:.1f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e04_summary.json"
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

    print(f"[E04] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
