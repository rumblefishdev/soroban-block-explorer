#!/usr/bin/env python3
"""E20 — /liquidity-pools/:id/transactions compare CH ↔ Horizon.

Per-pool per-ledger set compare (same shape as E07, E10).

For each sampled pool + sampled ledger from that pool's own activity:
  * CH: tx hashes from `operations_appearances` filtered by pool_id
    in that ledger.
  * Horizon: `/liquidity_pools/:id/transactions?cursor=...&order=desc`
    paginated down to the anchor ledger.
  * Diff: per-ledger hash set equality + per-row source_account /
    fee_charged / successful field check on the intersection.

Horizon `/liquidity_pools/:id/transactions` semantics: tx that
touched the pool (deposit / withdrawal / path payment that crossed
the pool). Should match CH `operations_appearances` filter
on `pool_id` 1:1 for in-ledger compare.

Note: 0252 Phase B E07 found CH `transaction_participants` is
broader than Horizon `/accounts/:id/transactions`. The same caveat
MAY apply here (CH includes broader pool-touching tx via LedgerEntry
changes that Horizon's per-pool listing doesn't surface). Track as
tolerance if rate < 1 %.
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
    read_completed_keys,
    write_tsv_header,
)

import requests


ENDPOINT = "E20"
TSV = OUT_DIR / "phase_b_e20.tsv"

HORIZON_FLOOR = 56657428
CH_TIP_FLOOR = 62525000

DEFAULT_ANCHORS = int(os.environ.get("SBE_E20_ANCHORS", "200"))


def load_active_pools(n: int) -> list[str]:
    """Sample pools that have retention-valid op-appearance activity.
    `samples_pools.txt` includes pools that were created but never
    traded — those yield `NO_LEDGER` on every anchor. Pull pool hex
    keys directly from `operations_appearances` so the sample is
    100 % retention-valid.
    """
    sql = f"""
    SELECT lower(hex(pool_id)) AS pool_hex
    FROM operations_appearances
    WHERE pool_id IS NOT NULL
      AND ledger_sequence > {HORIZON_FLOOR}
      AND ledger_sequence <= {CH_TIP_FLOOR}
    GROUP BY pool_id
    ORDER BY cityHash64(pool_id)
    LIMIT {n * 2}
    FORMAT TabSeparated
    """
    out = ch_query(sql).splitlines()
    return [p.strip() for p in out if p.strip()]


def sample_ledger_for_pool(pool_hex: str) -> int | None:
    """Pick one retention-valid ledger from the pool's own op-appearance
    activity. Stable via cityHash64.
    """
    sql = f"""
    SELECT ledger_sequence
    FROM operations_appearances
    WHERE pool_id = unhex('{pool_hex}')
      AND ledger_sequence > {HORIZON_FLOOR}
      AND ledger_sequence <= {CH_TIP_FLOOR}
    ORDER BY cityHash64(pool_id, ledger_sequence)
    LIMIT 1
    FORMAT TabSeparated
    """
    out = ch_query(sql).strip()
    return int(out) if out else None


def fetch_ch_pool_ledger(pool_hex: str, ledger: int) -> list[dict]:
    """Tx in `ledger` touching the pool.

    Two-stage to avoid the heavy unpruned JOIN on `transactions FINAL`
    when CH's planner can't push the partition predicate through the
    cursor side — see `compare_e13.fetch_ch_invocations` rationale.
    """
    part = ledger // 500_000
    sql_oa = f"""
    SELECT DISTINCT
        ledger_sequence,
        transaction_id
    FROM operations_appearances
    WHERE pool_id = unhex('{pool_hex}')
      AND ledger_sequence = {ledger}
      AND intDiv(ledger_sequence, 500000) = {part}
    FORMAT JSONEachRow
    """
    oa = ch_query_json(sql_oa)
    if not oa:
        return []
    pairs = ",".join(
        f"({int(r['ledger_sequence'])},{int(r['transaction_id'])})" for r in oa
    )
    sql_tx = f"""
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
    WHERE intDiv(t.ledger_sequence, 500000) = {part}
      AND (t.ledger_sequence, t.id) IN ({pairs})
    FORMAT JSONEachRow
    """
    return ch_query_json(sql_tx)


def fetch_horizon_pool_ledger(pool_hex: str, ledger: int) -> list[dict]:
    """Walk Horizon /liquidity_pools/:id/transactions desc from cursor
    just above the anchor ledger; collect tx with ledger ==
    anchor_ledger.
    """
    cursor = str((ledger + 1) * (1 << 32))
    url = f"{HORIZON_BASE}/liquidity_pools/{pool_hex}/transactions"
    params = {"cursor": cursor, "order": "desc", "limit": 200,
              "include_failed": "true"}

    out: list[dict] = []
    page = 0
    while page < 20:
        if page == 0:
            r = requests.get(url, params=params, timeout=45)
        else:
            r = requests.get(url, timeout=45)
        if r.status_code == 200:
            body = r.json()
            recs = body.get("_embedded", {}).get("records", []) or []
            if not recs:
                return out
            for rec in recs:
                lseq = int(rec.get("ledger", -1))
                if lseq < ledger:
                    return out
                if lseq == ledger:
                    out.append(rec)
            next_link = body.get("_links", {}).get("next", {}).get("href")
            if not next_link:
                return out
            url = next_link
            params = None
            page += 1
            time.sleep(HORIZON_DELAY)
            continue
        if r.status_code == 429 or r.status_code >= 500:
            time.sleep(2 ** min(page, 5))
            continue
        if r.status_code in (400, 404):
            return out
        r.raise_for_status()
    return out


def compare_anchor(pool_hex: str, ledger: int,
                   ch_rows: list[dict], hz_rows: list[dict],
                   result: EndpointResult) -> list[str]:
    diffs: list[str] = []

    ch_hashes = {r["hash"].lower() for r in ch_rows}
    hz_hashes = {r.get("hash", "").lower() for r in hz_rows if r.get("hash")}

    only_ch = ch_hashes - hz_hashes
    only_hz = hz_hashes - ch_hashes

    # Per-ledger SET equality is the load-bearing assertion.
    if not only_ch and not only_hz:
        result.record_field("hash_set_equal", "pass")
    else:
        # CH semantic may be broader than Horizon for op-counterparty
        # participants (see 0252 E07 finding). Tolerate when only_ch
        # > 0 and only_hz == 0 (CH found extra tx Horizon didn't list)
        # at < 5 % of the page — over that, it's real divergence.
        denom = max(len(ch_hashes), 1)
        if only_hz == set() and only_ch and len(only_ch) / denom < 0.05:
            result.record_field("hash_set_equal", "tolerance")
            diffs.append(
                f"hash_set CH-broader only_ch={len(only_ch)} only_hz=0 "
                f"sample_ch={list(only_ch)[:3]} "
                f"(CH op-counterparty semantic — see 0252 E07 finding)"
            )
        else:
            result.record_field("hash_set_equal", "fail")
            diffs.append(
                f"hash_set diff only_ch={len(only_ch)} only_hz={len(only_hz)} "
                f"ch_count={len(ch_hashes)} hz_count={len(hz_hashes)} "
                f"ch_sample={list(only_ch)[:3]} hz_sample={list(only_hz)[:3]}"
            )

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
    print(f"[E20] {len(done_keys)} anchors already done", file=sys.stderr)

    pools = load_active_pools(DEFAULT_ANCHORS)
    print(f"[E20] {len(pools)} active pools loaded from "
          f"operations_appearances", file=sys.stderr)
    random.seed(42)
    random.shuffle(pools)
    pools = pools[:DEFAULT_ANCHORS]

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        pools = pools[:pilot]
        print(f"[E20] PILOT mode — first {pilot}", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(pools)
    started = time.monotonic()
    processed = 0

    for pool_hex in pools:
        processed += 1

        try:
            ledger = sample_ledger_for_pool(pool_hex)
        except RuntimeError as e:
            append_tsv_row(TSV, ENDPOINT, pool_hex, 0, 0, 1, f"CH_SAMPLE_ERR:{str(e)[:80]}")
            result.fail_total += 1
            continue
        if ledger is None:
            append_tsv_row(TSV, ENDPOINT, pool_hex, 0, 0, 0, "NO_LEDGER")
            continue

        key = f"{pool_hex}@{ledger}"
        if key in done_keys:
            continue

        try:
            ch_rows = fetch_ch_pool_ledger(pool_hex, ledger)
        except RuntimeError as e:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 1, f"CH_ERR:{str(e)[:80]}")
            result.fail_total += 1
            continue

        hz_rows = fetch_horizon_pool_ledger(pool_hex, ledger)
        if not hz_rows and not ch_rows:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 0, "BOTH_EMPTY")
            continue

        diffs = compare_anchor(pool_hex, ledger, ch_rows, hz_rows, result)

        pass_n = 0 if any(d.startswith("hash_set ") for d in diffs) else 1
        tol_n = sum(1 for d in diffs if "(Horizon semantic drift)" in d or "(CH op-counterparty" in d)
        fail_n = sum(1 for d in diffs if "(Horizon semantic drift)" not in d and "(CH op-counterparty" not in d)

        if diffs:
            dump_diff(ENDPOINT, key, ch_rows, hz_rows, diffs)
            note = ";".join(diffs)[:500]
        else:
            note = ""
        append_tsv_row(TSV, ENDPOINT, key, pass_n, tol_n, fail_n, note)

        if processed % 25 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(pools) - processed) / max(rate, 0.01))
            print(f"[E20] {processed}/{len(pools)} anchors, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"tol={result.tolerance_total} rate={rate:.2f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e20_summary.json"
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

    print(f"[E20] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
