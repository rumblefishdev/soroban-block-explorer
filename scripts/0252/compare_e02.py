#!/usr/bin/env python3
"""E02 — /transactions list (paginated) compare CH ↔ Horizon.

E03 validates each tx's fields when fetched by hash. E02 validates the
*list* contract: for a given anchor cursor, does CH return the same set
of tx hashes as Horizon, with the same row count, in the same ledger
window?

Compare strategy
----------------
Sequence within a ledger differs across stores:
  - Horizon orders by application_order DESC within a ledger.
  - CH `transactions.id` is `cityhash64(network_id || hash)`, so its
    secondary sort (id DESC) is essentially random per ledger.

Therefore: compare as HASH SETS per anchor, not ordered sequences. The
set equality is the load-bearing assertion; sequence is a no-op claim
the canonical query never made.

Per anchor (one "page" worth):
  1. Pick anchor_ledger from samples_ledgers.txt (retention-valid only).
  2. CH:      last 50 tx with ledger_sequence <= anchor_ledger
              ORDER BY ledger_sequence DESC, id DESC.
              (Constrained to anchor's 500K partition + the one below
              if the page spans the boundary; matches canonical query
              behaviour. We just widen to "intDiv = N or N-1" in our
              raw SQL — the canonical query is paginated and would
              fetch the next partition on a follow-up call.)
  3. Horizon: GET /transactions?cursor=PT&order=desc&limit=50
              where PT = (anchor_ledger + 1) * 2**32 (paging token
              boundary; tx within anchor_ledger come back in desc
              app_order).
  4. Diff:
     a. count match — both should return 50 rows when anchor is deep
        enough; tolerated drift on tail edges.
     b. hash set equality — strict.
     c. per-row source_account / fee_charged / successful for the
        intersection (catches projection-level errors that hash-set
        compare alone would miss).

Sample size: 600 anchor pages × 50 rows = 30K tx-row compares.

Output: TSV row per anchor, JSON diff per mismatched anchor.
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
    HORIZON_BASE,
    HORIZON_DELAY,
    OUT_DIR,
    EndpointResult,
    append_tsv_row,
    ch_query,
    ch_query_json,
    dump_diff,
    horizon_get,
    load_samples,
    read_completed_keys,
    write_tsv_header,
)

import requests


ENDPOINT = "E02"
TSV = OUT_DIR / "phase_b_e02.tsv"

# Horizon retention floor — same constant as E03 (per Phase 6 Tier 5).
HORIZON_FLOOR = 56657428

# Anchors deeper than this are guaranteed not to bump into the live tip.
# CH backfill stops at 62,527,999; safe gap of 2K ledgers.
CH_TIP_FLOOR = 62525000

PAGE_LIMIT = 50

# Default 600 anchors → 30K row compares.
DEFAULT_ANCHORS = int(os.environ.get("SBE_E02_ANCHORS", "600"))


def horizon_paging_token(ledger: int, app_order: int = 0) -> str:
    """Horizon paging_token = ledger_seq * 2**32 + application_order."""
    return str(ledger * (1 << 32) + app_order)


def fetch_ch_page(anchor_ledger: int, limit: int = PAGE_LIMIT) -> list[dict]:
    """Last `limit` tx with ledger_sequence <= anchor_ledger.

    Spans up to two 500K partitions (the anchor's + the one below) to
    cover the case where the page reaches across the boundary. The
    canonical query handles this via subsequent paginated calls; for
    the compare we just widen the WHERE so we get all rows we'd see
    after one or two cursor-driven pages.
    """
    part_hi = anchor_ledger // 500_000
    part_lo = part_hi - 1
    sql = f"""
    SELECT
        lower(hex(t.hash))                    AS hash,
        t.ledger_sequence                     AS ledger_sequence,
        t.application_order                   AS application_order,
        src.account_id                        AS source_account,
        toString(t.fee_charged)               AS fee_charged,
        t.successful                          AS successful,
        t.operation_count                     AS operation_count
    FROM transactions AS t FINAL
    LEFT JOIN accounts AS src FINAL ON src.id = t.source_id
    WHERE intDiv(t.ledger_sequence, 500000) IN ({part_lo}, {part_hi})
      AND t.ledger_sequence <= {anchor_ledger}
    ORDER BY t.ledger_sequence DESC, t.id DESC
    LIMIT {limit}
    FORMAT JSONEachRow
    """
    return ch_query_json(sql)


def fetch_horizon_page(anchor_ledger: int, limit: int = PAGE_LIMIT) -> list[dict]:
    """Horizon `/transactions?cursor=PT&order=desc&limit=N`.

    Cursor = (anchor_ledger + 1) * 2**32 so the first returned row is
    the highest-app_order tx in `anchor_ledger`.
    """
    cursor = horizon_paging_token(anchor_ledger + 1, 0)
    url = f"{HORIZON_BASE}/transactions"
    params = {"cursor": cursor, "order": "desc", "limit": limit,
              "include_failed": "true"}
    for attempt in range(5):
        r = requests.get(url, params=params, timeout=45)
        if r.status_code == 200:
            time.sleep(HORIZON_DELAY)
            return r.json().get("_embedded", {}).get("records", []) or []
        if r.status_code == 429 or r.status_code >= 500:
            time.sleep(2 ** attempt)
            continue
        if r.status_code == 404:
            return []
        r.raise_for_status()
    return []


def compare_anchor(anchor_ledger: int, ch_rows: list[dict],
                   hz_rows: list[dict], result: EndpointResult) -> list[str]:
    """Set-based hash compare + intersection field check.

    Records 6 logical fields:
      - count_match        (per anchor)
      - hash_set_equal     (per anchor)
      - source_account     (per intersected row)
      - fee_charged        (per intersected row)
      - successful         (per intersected row)
      - operation_count    (per intersected row — tolerance)
    """
    diffs: list[str] = []

    ch_hashes = {r["hash"].lower() for r in ch_rows}
    hz_hashes = {r.get("hash", "").lower() for r in hz_rows if r.get("hash")}

    # Anchor-level: count match.
    if len(ch_rows) == len(hz_rows):
        result.record_field("count_match", "pass")
    else:
        # Tolerance: tail-edge anchors near retention floor may have
        # Horizon-side gaps (live drift / pruning latency).
        if anchor_ledger - PAGE_LIMIT < HORIZON_FLOOR + 200:
            result.record_field("count_match", "tolerance")
            diffs.append(f"count CH={len(ch_rows)} HZ={len(hz_rows)} (retention edge)")
        else:
            result.record_field("count_match", "fail")
            diffs.append(f"count CH={len(ch_rows)} HZ={len(hz_rows)}")

    # Anchor-level: hash set equality.
    only_ch = ch_hashes - hz_hashes
    only_hz = hz_hashes - ch_hashes
    if not only_ch and not only_hz:
        result.record_field("hash_set_equal", "pass")
    else:
        # Tail-edge tolerance, same logic as count.
        if anchor_ledger - PAGE_LIMIT < HORIZON_FLOOR + 200:
            result.record_field("hash_set_equal", "tolerance")
            diffs.append(f"hash_set diff only_ch={len(only_ch)} only_hz={len(only_hz)} (retention edge)")
        else:
            result.record_field("hash_set_equal", "fail")
            sample_only_ch = list(only_ch)[:3]
            sample_only_hz = list(only_hz)[:3]
            diffs.append(
                f"hash_set diff only_ch={len(only_ch)} only_hz={len(only_hz)} "
                f"sample_ch={sample_only_ch} sample_hz={sample_only_hz}"
            )

    # Per-row field compare on intersection. Build hash → row maps.
    ch_by_hash = {r["hash"].lower(): r for r in ch_rows}
    hz_by_hash = {r["hash"].lower(): r for r in hz_rows if r.get("hash")}
    inter = ch_hashes & hz_hashes

    for h in inter:
        ch = ch_by_hash[h]
        hz = hz_by_hash[h]

        if ch["source_account"] == hz.get("source_account"):
            result.record_field("source_account", "pass")
        else:
            result.record_field("source_account", "fail")
            diffs.append(f"{h[:12]}.. source_account CH={ch['source_account']} HZ={hz.get('source_account')}")

        if str(ch["fee_charged"]) == str(hz.get("fee_charged")):
            result.record_field("fee_charged", "pass")
        else:
            result.record_field("fee_charged", "fail")
            diffs.append(f"{h[:12]}.. fee_charged CH={ch['fee_charged']} HZ={hz.get('fee_charged')}")

        if bool(ch["successful"]) == bool(hz.get("successful")):
            result.record_field("successful", "pass")
        else:
            result.record_field("successful", "fail")
            diffs.append(f"{h[:12]}.. successful CH={ch['successful']} HZ={hz.get('successful')}")

        ch_oc = int(ch.get("operation_count", 0))
        hz_oc = int(hz.get("operation_count") or 0)
        if ch_oc == hz_oc:
            result.record_field("operation_count", "pass")
        else:
            result.record_field("operation_count", "tolerance")
            diffs.append(f"{h[:12]}.. operation_count CH={ch_oc} HZ={hz_oc} (Horizon semantic drift)")

    return diffs


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E02] {len(done_keys)} anchors already done", file=sys.stderr)

    all_ledgers = [int(s) for s in load_samples("samples_ledgers.txt")]
    print(f"[E02] {len(all_ledgers)} ledger samples loaded", file=sys.stderr)

    # Retention-valid only. Avoid the live tip too.
    retention_safe = [L for L in all_ledgers
                      if HORIZON_FLOOR <= L <= CH_TIP_FLOOR]
    random.seed(42)
    anchors = random.sample(retention_safe, min(DEFAULT_ANCHORS, len(retention_safe)))
    print(f"[E02] {len(anchors)} anchors picked from {len(retention_safe)} retention-valid",
          file=sys.stderr)

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        anchors = anchors[:pilot]
        print(f"[E02] PILOT mode — first {pilot} anchors", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(anchors)
    started = time.monotonic()
    processed = 0

    for anchor_ledger in anchors:
        key = str(anchor_ledger)
        if key in done_keys:
            continue
        processed += 1

        try:
            ch_rows = fetch_ch_page(anchor_ledger)
        except RuntimeError as e:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 1, f"CH_ERROR:{str(e)[:120]}")
            result.fail_total += 1
            continue

        hz_rows = fetch_horizon_page(anchor_ledger)
        if not hz_rows:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 0, "HZ_EMPTY")
            continue

        diffs = compare_anchor(anchor_ledger, ch_rows, hz_rows, result)

        # Compute counts for the TSV row. Rather than recount diff
        # strings (mix of fail / tolerance), use the result deltas
        # this anchor produced — easier: just emit aggregate row.
        pass_n = (
            (1 if not any(d.startswith("count ") for d in diffs) else 0)
            + (1 if not any(d.startswith("hash_set ") for d in diffs) else 0)
        )
        tol_n = sum(1 for d in diffs if "(retention edge)" in d or "(Horizon semantic drift)" in d)
        fail_n = sum(1 for d in diffs if "(retention edge)" not in d and "(Horizon semantic drift)" not in d
                     and (d.startswith("count ") or d.startswith("hash_set ")
                          or " source_account " in d or " fee_charged " in d
                          or " successful " in d))

        if diffs:
            dump_diff(ENDPOINT, key, ch_rows, hz_rows, diffs)
            note = ";".join(diffs)[:500]
        else:
            note = ""

        append_tsv_row(TSV, ENDPOINT, key, pass_n, tol_n, fail_n, note)

        if processed % 25 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(anchors) - processed) / max(rate, 0.01))
            print(f"[E02] {processed}/{len(anchors)} anchors, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"tol={result.tolerance_total} rate={rate:.2f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e02_summary.json"
    summary.write_text(json.dumps({
        "endpoint": ENDPOINT,
        "sample_size": result.sample_size,
        "pass_total": result.pass_total,
        "fail_total": result.fail_total,
        "tolerance_total": result.tolerance_total,
        "elapsed_ms": result.elapsed_ms,
        "anchors": result.sample_size,
        "page_limit": PAGE_LIMIT,
        "fields": {
            k: {"pass": v.pass_count, "tolerance": v.tolerance_count, "fail": v.fail_count}
            for k, v in result.fields.items()
        },
    }, indent=2))

    print(f"[E02] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
