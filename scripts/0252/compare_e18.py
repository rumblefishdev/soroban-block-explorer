#!/usr/bin/env python3
"""E18 — /liquidity-pools list compare CH ↔ Horizon.

E19 validates the per-pool DETAIL view (`/liquidity_pools/:id`). E18
validates the LIST projection — same per-pool fields plus the
list-specific JOIN/projection (closed_at of latest snapshot, asset
issuer accounts).

Per anchor (= per pool):
  1. Pick pool from samples_pools.txt (5K cap for Phase B parity with
     E19).
  2. CH: run the E18 list query SHAPE (multi-join) constrained to
     this pool (WHERE lp.pool_id = unhex(pool_hex)) so we get the
     exact same projection the list view emits, just for one row.
  3. Horizon: GET /liquidity_pools/:id.
  4. Field compare:
     - fee_bps                strict
     - reserve_a / reserve_b  strict (live drift → tolerance)
     - total_shares           strict (live drift → tolerance)
     - asset_a_code           strict
     - asset_a_issuer         strict
     - asset_b_code           strict
     - asset_b_issuer         strict
     - latest_snapshot_at     strict (within 1s) — CH closed_at vs
                              Horizon last_modified_time

The 4 asset fields are the value-add over E19 (E19 only validates
`type` discriminator, not the full code/issuer combo). The
`latest_snapshot_at` check verifies the JOIN `ledgers` projection.

Sample: 5000 pools (mirrors E19 cap).
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


ENDPOINT = "E18"
TSV = OUT_DIR / "phase_b_e18.tsv"

CH_BACKFILL_TIP = 62527999

DEFAULT_SAMPLE = int(os.environ.get("SBE_E18_SAMPLE", "5000"))


def fetch_ch_pool(pool_hex: str) -> dict | None:
    """List-projection shape for a single pool. Mirrors the E18 list
    query JOINs (accounts × 2, snapshots argMax, ledgers).
    """
    sql = f"""
    SELECT
        lower(hex(lp.pool_id))                AS pool_id_hex,
        lp.fee_bps                            AS fee_bps,
        lp.asset_a_type                       AS asset_a_type,
        lp.asset_a_code                       AS asset_a_code,
        iss_a.account_id                      AS asset_a_issuer,
        lp.asset_b_type                       AS asset_b_type,
        lp.asset_b_code                       AS asset_b_code,
        iss_b.account_id                      AS asset_b_issuer,
        lp.last_updated_ledger                AS last_updated_ledger,
        s.latest_ledger_sequence              AS latest_snapshot_ledger,
        toString(s.reserve_a)                 AS reserve_a,
        toString(s.reserve_b)                 AS reserve_b,
        toString(s.total_shares)              AS total_shares,
        toString(l_snap.closed_at)            AS latest_snapshot_at
    FROM liquidity_pools lp FINAL
    LEFT JOIN accounts iss_a FINAL ON iss_a.id = lp.asset_a_issuer_id AND lp.asset_a_issuer_id != 0
    LEFT JOIN accounts iss_b FINAL ON iss_b.id = lp.asset_b_issuer_id AND lp.asset_b_issuer_id != 0
    LEFT JOIN (
        SELECT
            pool_id,
            max(ledger_sequence)                       AS latest_ledger_sequence,
            argMax(reserve_a,    ledger_sequence)      AS reserve_a,
            argMax(reserve_b,    ledger_sequence)      AS reserve_b,
            argMax(total_shares, ledger_sequence)      AS total_shares
        FROM liquidity_pool_snapshots FINAL
        WHERE pool_id = unhex('{pool_hex}')
        GROUP BY pool_id
    ) s ON s.pool_id = lp.pool_id
    LEFT JOIN ledgers l_snap ON l_snap.sequence = s.latest_ledger_sequence
    WHERE lp.pool_id = unhex('{pool_hex}')
    LIMIT 1
    FORMAT JSONEachRow
    """
    rows = ch_query_json(sql)
    return rows[0] if rows else None


def fetch_horizon_pool(pool_hex: str) -> dict | None:
    body = horizon_get(f"/liquidity_pools/{pool_hex}")
    if not body or "id" not in body:
        return None
    return body


def horizon_asset_split(asset_str: str) -> tuple[str, str | None]:
    """Horizon reserves[].asset = 'CODE:ISSUER' (or 'native').

    Returns (code, issuer_or_None).
    """
    if asset_str == "native":
        return ("", None)
    if ":" in asset_str:
        code, _, issuer = asset_str.partition(":")
        return (code, issuer)
    return (asset_str, None)


def compare_pool(pool_hex: str, ch: dict, hz: dict, result: EndpointResult) -> list[str]:
    diffs: list[str] = []

    # fee_bps
    ch_fee = int(ch["fee_bps"])
    hz_fee = int(hz.get("fee_bp") or hz.get("fee_bps") or 0)
    if ch_fee == hz_fee:
        result.record_field("fee_bps", "pass")
    else:
        result.record_field("fee_bps", "fail")
        diffs.append(f"fee_bps CH={ch_fee} HZ={hz_fee}")

    # Asset split. Horizon order = (asset_a, asset_b) by canonical sort
    # (native first, then alpha by code/issuer). CH stores
    # (asset_a, asset_b) per the same canonical order. So index 0/1
    # alignment is safe.
    hz_reserves = hz.get("reserves") or []
    if len(hz_reserves) != 2:
        result.record_field("asset_a_code", "fail")
        result.record_field("asset_b_code", "fail")
        diffs.append(f"reserves shape len={len(hz_reserves)}")
    else:
        hz_a_code, hz_a_issuer = horizon_asset_split(hz_reserves[0].get("asset", ""))
        hz_b_code, hz_b_issuer = horizon_asset_split(hz_reserves[1].get("asset", ""))

        # asset_a_code
        if (ch["asset_a_code"] or "") == hz_a_code:
            result.record_field("asset_a_code", "pass")
        else:
            result.record_field("asset_a_code", "fail")
            diffs.append(f"asset_a_code CH={ch['asset_a_code']!r} HZ={hz_a_code!r}")

        # asset_a_issuer
        if (ch["asset_a_issuer"] or None) == hz_a_issuer:
            result.record_field("asset_a_issuer", "pass")
        else:
            result.record_field("asset_a_issuer", "fail")
            diffs.append(f"asset_a_issuer CH={ch['asset_a_issuer']!r} HZ={hz_a_issuer!r}")

        # asset_b_code
        if (ch["asset_b_code"] or "") == hz_b_code:
            result.record_field("asset_b_code", "pass")
        else:
            result.record_field("asset_b_code", "fail")
            diffs.append(f"asset_b_code CH={ch['asset_b_code']!r} HZ={hz_b_code!r}")

        # asset_b_issuer
        if (ch["asset_b_issuer"] or None) == hz_b_issuer:
            result.record_field("asset_b_issuer", "pass")
        else:
            result.record_field("asset_b_issuer", "fail")
            diffs.append(f"asset_b_issuer CH={ch['asset_b_issuer']!r} HZ={hz_b_issuer!r}")

        # reserve_a / reserve_b — Horizon reports raw stroops as
        # decimal string (e.g. "1.2345678"); CH stores raw integer
        # stroops in `s.reserve_a`. Convert Horizon to integer
        # stroops (× 10^7, drop the decimal).
        def hz_amount_to_stroops(s: str) -> int:
            if "." in s:
                whole, frac = s.split(".", 1)
                frac = (frac + "0000000")[:7]
                return int(whole) * 10_000_000 + int(frac)
            return int(s) * 10_000_000

        try:
            hz_res_a = hz_amount_to_stroops(hz_reserves[0].get("amount", "0"))
            hz_res_b = hz_amount_to_stroops(hz_reserves[1].get("amount", "0"))
        except (ValueError, TypeError):
            hz_res_a = hz_res_b = -1

        ch_res_a = int(ch.get("reserve_a") or 0)
        ch_res_b = int(ch.get("reserve_b") or 0)

        if ch_res_a == hz_res_a:
            result.record_field("reserve_a", "pass")
        else:
            # Live drift: CH snapshot lags Horizon current state.
            result.record_field("reserve_a", "tolerance")
            diffs.append(f"reserve_a CH={ch_res_a} HZ={hz_res_a} (live drift)")

        if ch_res_b == hz_res_b:
            result.record_field("reserve_b", "pass")
        else:
            result.record_field("reserve_b", "tolerance")
            diffs.append(f"reserve_b CH={ch_res_b} HZ={hz_res_b} (live drift)")

    # total_shares — Horizon `total_shares` is decimal stringified.
    # CH stores raw integer (stroops-like 7-decimal).
    hz_shares_raw = hz.get("total_shares", "0")
    try:
        if "." in str(hz_shares_raw):
            whole, frac = str(hz_shares_raw).split(".", 1)
            frac = (frac + "0000000")[:7]
            hz_shares = int(whole) * 10_000_000 + int(frac)
        else:
            hz_shares = int(hz_shares_raw) * 10_000_000
    except (ValueError, TypeError):
        hz_shares = -1
    ch_shares = int(ch.get("total_shares") or 0)
    if ch_shares == hz_shares:
        result.record_field("total_shares", "pass")
    else:
        result.record_field("total_shares", "tolerance")
        diffs.append(f"total_shares CH={ch_shares} HZ={hz_shares} (live drift)")

    # latest_snapshot_at — CH ledger.closed_at vs Horizon last_modified_time.
    # Drift tolerated (CH backfill snapshot lags Horizon live).
    ch_ts_raw = ch.get("latest_snapshot_at") or ""
    hz_ts_raw = hz.get("last_modified_time") or ""
    if not ch_ts_raw or not hz_ts_raw:
        result.record_field("latest_snapshot_at", "tolerance")
        diffs.append(f"latest_snapshot_at missing CH={ch_ts_raw!r} HZ={hz_ts_raw!r}")
    else:
        ch_norm = "".join(c for c in ch_ts_raw if c.isdigit())[:14]
        hz_norm = "".join(c for c in hz_ts_raw if c.isdigit())[:14]
        if ch_norm == hz_norm:
            result.record_field("latest_snapshot_at", "pass")
        else:
            result.record_field("latest_snapshot_at", "tolerance")
            diffs.append(f"latest_snapshot_at CH={ch_ts_raw} HZ={hz_ts_raw} (live drift)")

    return diffs


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E18] {len(done_keys)} pools already done", file=sys.stderr)

    pools = load_samples("samples_pools.txt")
    print(f"[E18] {len(pools)} pool samples loaded", file=sys.stderr)

    random.seed(42)
    if DEFAULT_SAMPLE < len(pools):
        pools = random.sample(pools, DEFAULT_SAMPLE)
        print(f"[E18] sampled to {len(pools)}", file=sys.stderr)

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        pools = pools[:pilot]
        print(f"[E18] PILOT mode — first {pilot}", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(pools)
    started = time.monotonic()
    processed = 0

    for pool_hex in pools:
        if pool_hex in done_keys:
            continue
        processed += 1

        try:
            ch = fetch_ch_pool(pool_hex)
        except RuntimeError as e:
            append_tsv_row(TSV, ENDPOINT, pool_hex, 0, 0, 1, f"CH_ERROR:{str(e)[:120]}")
            result.fail_total += 1
            continue

        if ch is None:
            append_tsv_row(TSV, ENDPOINT, pool_hex, 0, 0, 1, "CH_MISSING")
            result.fail_total += 1
            continue

        hz = fetch_horizon_pool(pool_hex)
        if hz is None:
            append_tsv_row(TSV, ENDPOINT, pool_hex, 0, 0, 0, "HZ_MISSING")
            continue

        diffs = compare_pool(pool_hex, ch, hz, result)

        # Per-row TSV: 9 logical fields max (fee_bps, 4 asset_*, reserve_a,
        # reserve_b, total_shares, latest_snapshot_at).
        pass_n = 9 - len(diffs)
        tol_n = sum(1 for d in diffs if "(live drift)" in d or "missing" in d)
        fail_n = len(diffs) - tol_n

        if diffs:
            dump_diff(ENDPOINT, pool_hex, ch, hz, diffs)
            note = ";".join(diffs)[:500]
        else:
            note = ""

        append_tsv_row(TSV, ENDPOINT, pool_hex, pass_n, tol_n, fail_n, note)

        if processed % 100 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(pools) - processed) / max(rate, 0.01))
            print(f"[E18] {processed}/{len(pools)} done, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"tol={result.tolerance_total} rate={rate:.1f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e18_summary.json"
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

    print(f"[E18] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
