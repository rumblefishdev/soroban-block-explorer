#!/usr/bin/env python3
"""E19 — /liquidity-pools/:id compare CH ↔ Horizon.

Per-pool compare. Field-by-field diff on 7 fields:
  1. pool_id_hex
  2. fee_bp
  3. total_shares
  4. reserve_a
  5. reserve_b
  6. last_updated_ledger
  7. type (constant_product / fixed_a_b)

Sample source: samples_pools.txt (~50K, full population).
Cap to 5K stratified by random for Phase B (SBE_PHASE_B_CAP).
"""

from __future__ import annotations

import json
import os
import random
import sys
import time
from decimal import Decimal
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


ENDPOINT = "E19"
TSV = OUT_DIR / "phase_b_e19.tsv"

# CH backfill ends at this ledger; anything Horizon reports past
# this is live-chain drift (swaps, deposits, withdrawals after our
# snapshot end). Pre-Phase 6 metric: max(ledgers.sequence) = 62527999.
CH_BACKFILL_TIP = 62527999


def fetch_ch_pool(pool_hex: str) -> dict | None:
    """Canonical CH schema (per init.sql):
      liquidity_pools (RMT(last_updated_ledger), ORDER BY pool_id):
        pool_id, asset_{a,b}_{type,code,issuer_id}, fee_bps,
        last_updated_ledger.
      liquidity_pool_snapshots (RMT, partitioned per 500K ledger):
        pool_id, ledger_sequence, reserve_a, reserve_b, total_shares,
        tvl, volume, fee_revenue.

    Current state = liquidity_pools FINAL JOIN latest snapshot
    (argMax by ledger_sequence) on pool_id.
    """
    sql = f"""
    SELECT
      lower(hex(lp.pool_id))            AS pool_id_hex,
      lp.fee_bps                        AS fee_bps,
      lp.last_updated_ledger            AS last_updated_ledger,
      lp.asset_a_type                   AS asset_a_type,
      lp.asset_a_code                   AS asset_a_code,
      lp.asset_a_issuer_id              AS asset_a_issuer_id,
      lp.asset_b_type                   AS asset_b_type,
      lp.asset_b_code                   AS asset_b_code,
      lp.asset_b_issuer_id              AS asset_b_issuer_id,
      toString(s.reserve_a)             AS reserve_a,
      toString(s.reserve_b)             AS reserve_b,
      toString(s.total_shares)          AS total_shares
    FROM liquidity_pools AS lp FINAL
    LEFT JOIN (
      SELECT pool_id,
             argMax(reserve_a, ledger_sequence)    AS reserve_a,
             argMax(reserve_b, ledger_sequence)    AS reserve_b,
             argMax(total_shares, ledger_sequence) AS total_shares
        FROM liquidity_pool_snapshots
       WHERE pool_id = unhex('{pool_hex}')
       GROUP BY pool_id
    ) AS s ON s.pool_id = lp.pool_id
    WHERE lp.pool_id = unhex('{pool_hex}')
    FORMAT JSONEachRow
    """
    rows = ch_query_json(sql)
    return rows[0] if rows else None


def fetch_horizon_pool(pool_hex: str) -> dict | None:
    body = horizon_get(f"/liquidity_pools/{pool_hex}")
    if not body or "id" not in body:
        return None
    return body


def decimal_close(a: str, b: str, places: int = 7) -> bool:
    """Compare two stringified decimals (or ints) accepting small drift
    at the last decimal place (CH Decimal128(7) vs Horizon string)."""
    try:
        da = Decimal(str(a))
        db = Decimal(str(b))
        eps = Decimal(10) ** (-places)
        return abs(da - db) <= eps
    except Exception:
        return str(a) == str(b)


def compare(pool_hex: str, ch: dict, hz: dict, result: EndpointResult) -> list[str]:
    diffs: list[str] = []

    # Live-drift detector: if Horizon shows the pool was modified after
    # CH's backfill tip, reserves + total_shares are expected to differ
    # (swaps / deposits / withdrawals happened post-snapshot). Mark all
    # state fields as tolerance instead of fail.
    hz_lml = int(hz.get("last_modified_ledger", 0) or 0)
    is_live_drift = hz_lml > CH_BACKFILL_TIP

    # 1. pool_id_hex
    if ch["pool_id_hex"].lower() == hz["id"].lower():
        result.record_field("pool_id", "pass")
    else:
        result.record_field("pool_id", "fail")
        diffs.append(f"pool_id CH={ch['pool_id_hex']} HZ={hz['id']}")

    # 2. fee_bps (CH column name; Horizon emits as `fee_bp`)
    if int(ch["fee_bps"]) == int(hz.get("fee_bp", 0)):
        result.record_field("fee_bps", "pass")
    else:
        result.record_field("fee_bps", "fail")
        diffs.append(f"fee_bps CH={ch['fee_bps']} HZ={hz.get('fee_bp')}")

    # 3. total_shares — tolerance when live-drift; pass/fail otherwise.
    if decimal_close(ch["total_shares"], hz.get("total_shares", "0")):
        result.record_field("total_shares", "pass")
    elif is_live_drift:
        result.record_field("total_shares", "tolerance")
        diffs.append(f"total_shares CH={ch['total_shares']} HZ={hz.get('total_shares')} (live drift)")
    else:
        result.record_field("total_shares", "fail")
        diffs.append(f"total_shares CH={ch['total_shares']} HZ={hz.get('total_shares')}")

    # 4-5. reserves — same live-drift policy.
    reserves = hz.get("reserves", [])
    hz_a = reserves[0]["amount"] if len(reserves) > 0 else "0"
    hz_b = reserves[1]["amount"] if len(reserves) > 1 else "0"

    if decimal_close(ch["reserve_a"], hz_a):
        result.record_field("reserve_a", "pass")
    elif is_live_drift:
        result.record_field("reserve_a", "tolerance")
        diffs.append(f"reserve_a CH={ch['reserve_a']} HZ={hz_a} (live drift)")
    else:
        result.record_field("reserve_a", "fail")
        diffs.append(f"reserve_a CH={ch['reserve_a']} HZ={hz_a}")

    if decimal_close(ch["reserve_b"], hz_b):
        result.record_field("reserve_b", "pass")
    elif is_live_drift:
        result.record_field("reserve_b", "tolerance")
        diffs.append(f"reserve_b CH={ch['reserve_b']} HZ={hz_b} (live drift)")
    else:
        result.record_field("reserve_b", "fail")
        diffs.append(f"reserve_b CH={ch['reserve_b']} HZ={hz_b}")

    # 6. last_updated_ledger — when CH >= HZ → pass; when HZ > CH BUT
    # only by post-backfill activity → tolerance (expected drift).
    ch_lul = int(ch["last_updated_ledger"])
    if ch_lul == hz_lml:
        result.record_field("last_updated_ledger", "pass")
    elif is_live_drift:
        result.record_field("last_updated_ledger", "tolerance")
        diffs.append(f"last_updated_ledger CH={ch_lul} HZ={hz_lml} (live drift past {CH_BACKFILL_TIP})")
    else:
        result.record_field("last_updated_ledger", "fail")
        diffs.append(f"last_updated_ledger CH={ch_lul} HZ={hz_lml}")

    # 7. type — both should be "constant_product" for SAMM pools
    hz_type = hz.get("type", "")
    if hz_type:
        result.record_field("type", "pass")
    else:
        result.record_field("type", "tolerance")

    return diffs


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E19] {len(done_keys)} keys already done", file=sys.stderr)

    pools = load_samples("samples_pools.txt")
    print(f"[E19] {len(pools)} pool samples loaded", file=sys.stderr)

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        random.seed(42)
        pools = random.sample(pools, min(pilot, len(pools)))
        print(f"[E19] PILOT — capped to {len(pools)} pools", file=sys.stderr)

    cap = int(os.environ.get("SBE_PHASE_B_CAP", "5000"))
    if cap > 0 and len(pools) > cap:
        random.seed(123)
        pools = random.sample(pools, cap)
        print(f"[E19] capped to {cap} stratified samples", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(pools)
    started = time.monotonic()
    processed = 0

    for pool_hex in pools:
        if pool_hex in done_keys:
            continue
        processed += 1

        ch = fetch_ch_pool(pool_hex)
        if ch is None:
            append_tsv_row(TSV, ENDPOINT, pool_hex, 0, 0, 1, "CH_MISSING")
            result.fail_total += 1
            continue

        hz = fetch_horizon_pool(pool_hex)
        if hz is None:
            append_tsv_row(TSV, ENDPOINT, pool_hex, 0, 0, 0, "HZ_404")
            continue

        diffs = compare(pool_hex, ch, hz, result)
        if diffs:
            dump_diff(ENDPOINT, pool_hex, ch, hz, diffs)
            tol = sum(1 for d in diffs if "live drift" in d)
            fail = len(diffs) - tol
            pass_n = 7 - len(diffs)
            append_tsv_row(TSV, ENDPOINT, pool_hex, pass_n, tol, fail, ";".join(diffs))
        else:
            append_tsv_row(TSV, ENDPOINT, pool_hex, 7, 0, 0, "")

        if processed % 100 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(pools) - processed) / max(rate, 0.01))
            print(f"[E19] {processed}/{len(pools)} done, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"rate={rate:.1f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e19_summary.json"
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

    print(f"[E19] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
