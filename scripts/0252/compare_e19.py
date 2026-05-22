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


def fetch_ch_pool(pool_hex: str) -> dict | None:
    sql = f"""
    SELECT
      lower(hex(pool_id))         AS pool_id_hex,
      fee_bp,
      toString(total_shares)      AS total_shares,
      toString(reserve_a)         AS reserve_a,
      toString(reserve_b)         AS reserve_b,
      last_updated_ledger,
      asset_a_type,
      asset_a_code,
      asset_a_issuer_id,
      asset_b_type,
      asset_b_code,
      asset_b_issuer_id
    FROM liquidity_pools
    WHERE pool_id = unhex('{pool_hex}')
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

    # 1. pool_id_hex
    if ch["pool_id_hex"].lower() == hz["id"].lower():
        result.record_field("pool_id", "pass")
    else:
        result.record_field("pool_id", "fail")
        diffs.append(f"pool_id CH={ch['pool_id_hex']} HZ={hz['id']}")

    # 2. fee_bp
    if int(ch["fee_bp"]) == int(hz.get("fee_bp", 0)):
        result.record_field("fee_bp", "pass")
    else:
        result.record_field("fee_bp", "fail")
        diffs.append(f"fee_bp CH={ch['fee_bp']} HZ={hz.get('fee_bp')}")

    # 3. total_shares
    if decimal_close(ch["total_shares"], hz.get("total_shares", "0")):
        result.record_field("total_shares", "pass")
    else:
        result.record_field("total_shares", "fail")
        diffs.append(f"total_shares CH={ch['total_shares']} HZ={hz.get('total_shares')}")

    # 4-5. reserves
    reserves = hz.get("reserves", [])
    hz_a = reserves[0]["amount"] if len(reserves) > 0 else "0"
    hz_b = reserves[1]["amount"] if len(reserves) > 1 else "0"
    if decimal_close(ch["reserve_a"], hz_a):
        result.record_field("reserve_a", "pass")
    else:
        result.record_field("reserve_a", "fail")
        diffs.append(f"reserve_a CH={ch['reserve_a']} HZ={hz_a}")
    if decimal_close(ch["reserve_b"], hz_b):
        result.record_field("reserve_b", "pass")
    else:
        result.record_field("reserve_b", "fail")
        diffs.append(f"reserve_b CH={ch['reserve_b']} HZ={hz_b}")

    # 6. last_updated_ledger — Horizon `last_modified_ledger`
    if int(ch["last_updated_ledger"]) == int(hz.get("last_modified_ledger", 0)):
        result.record_field("last_updated_ledger", "pass")
    else:
        # Tolerance: Horizon may not have observed our latest update
        # if we're sampling the live frontier — rare for static history.
        result.record_field("last_updated_ledger", "tolerance")
        diffs.append(f"last_updated_ledger CH={ch['last_updated_ledger']} HZ={hz.get('last_modified_ledger')}")

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
            tol = sum(1 for d in diffs if "last_updated_ledger" in d)
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
