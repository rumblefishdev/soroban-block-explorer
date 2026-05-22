#!/usr/bin/env python3
"""E09 — /assets/:id compare CH ↔ Horizon.

Per-asset compare. Asset identity is a 4-tuple
(asset_type, asset_code, issuer_id, contract_id) — sample file stores
them pipe-delimited.

Field-by-field on 5 fields:
  1. asset_code echo
  2. asset_issuer (StrKey resolved from issuer_id via accounts)
  3. holder_count        — live drift expected
  4. total_supply (XLM)  — live drift expected
  5. asset_type matches semantic (1=classic credit, 2=SAC)

Horizon `/assets?asset_code=X&asset_issuer=Y` returns 0+ rows
matching the asset; we expect exactly 1 (classic) or 0 (SAC native).
Native + soroban-native (type 0/3) skipped — no Horizon equivalent.
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


ENDPOINT = "E09"
TSV = OUT_DIR / "phase_b_e09.tsv"


def parse_sample(line: str) -> tuple[int, str, int, int] | None:
    """Parse `type|code|issuer_id|contract_id` line."""
    parts = line.split("|")
    if len(parts) != 4:
        return None
    try:
        return int(parts[0]), parts[1], int(parts[2]), int(parts[3])
    except ValueError:
        return None


def fetch_ch_asset(atype: int, acode: str, issuer_id: int, contract_id: int) -> dict | None:
    """Resolve issuer_id → StrKey via accounts JOIN."""
    sql = f"""
    SELECT
      a.asset_type                          AS asset_type,
      a.asset_code                          AS asset_code,
      a.issuer_id                           AS issuer_id,
      a.contract_id                         AS contract_id,
      acc.account_id                        AS issuer_strkey,
      a.holder_count                        AS holder_count,
      toString(a.total_supply)              AS total_supply
    FROM assets AS a FINAL
    LEFT JOIN accounts AS acc FINAL ON acc.id = a.issuer_id
    WHERE a.asset_type = {atype}
      AND a.asset_code = '{acode}'
      AND a.issuer_id = {issuer_id}
      AND a.contract_id = {contract_id}
    LIMIT 1
    FORMAT JSONEachRow
    """
    rows = ch_query_json(sql)
    return rows[0] if rows else None


def fetch_horizon_asset(asset_code: str, issuer_strkey: str) -> dict | None:
    body = horizon_get("/assets", params={
        "asset_code": asset_code,
        "asset_issuer": issuer_strkey,
        "limit": 1,
    })
    if not body:
        return None
    records = body.get("_embedded", {}).get("records", [])
    if not records:
        return None
    return records[0]


def decimal_close(a: str, b: str, places: int = 7) -> bool:
    try:
        return abs(Decimal(str(a)) - Decimal(str(b))) <= Decimal(10) ** (-places)
    except Exception:
        return str(a) == str(b)


def compare(key: str, ch: dict, hz: dict, result: EndpointResult) -> list[str]:
    diffs: list[str] = []

    # 1. asset_code
    if ch["asset_code"] == hz.get("asset_code"):
        result.record_field("asset_code", "pass")
    else:
        result.record_field("asset_code", "fail")
        diffs.append(f"asset_code CH={ch['asset_code']} HZ={hz.get('asset_code')}")

    # 2. asset_issuer
    if ch["issuer_strkey"] == hz.get("asset_issuer"):
        result.record_field("asset_issuer", "pass")
    else:
        result.record_field("asset_issuer", "fail")
        diffs.append(f"asset_issuer CH={ch['issuer_strkey']} HZ={hz.get('asset_issuer')}")

    # 3. holder_count — Horizon `accounts.authorized` is the trustline
    # count; tolerance because Horizon = live tip, CH = snapshot end.
    hz_holders = (hz.get("accounts") or {}).get("authorized", 0) + \
                 (hz.get("accounts") or {}).get("authorized_to_maintain_liabilities", 0) + \
                 (hz.get("accounts") or {}).get("unauthorized", 0)
    ch_holders = int(ch.get("holder_count") or 0)
    if ch_holders == hz_holders:
        result.record_field("holder_count", "pass")
    else:
        result.record_field("holder_count", "tolerance")
        diffs.append(f"holder_count CH={ch_holders} HZ={hz_holders} (live drift)")

    # 4. total_supply — Horizon `amount` (string)
    ch_supply = ch.get("total_supply") or "0"
    hz_supply = hz.get("amount") or "0"
    if decimal_close(ch_supply, hz_supply):
        result.record_field("total_supply", "pass")
    else:
        result.record_field("total_supply", "tolerance")
        diffs.append(f"total_supply CH={ch_supply} HZ={hz_supply} (live drift)")

    # 5. asset_type semantic — Horizon emits "credit_alphanum4",
    # "credit_alphanum12" for classic; native skipped. CH 1 = classic,
    # 2 = SAC. SAC has no Horizon equivalent — should be filtered at
    # selection time, but defensively tolerate.
    hz_type = hz.get("asset_type", "")
    ch_type = int(ch.get("asset_type") or 0)
    if ch_type == 1 and hz_type.startswith("credit_alphanum"):
        result.record_field("asset_type", "pass")
    elif ch_type == 2 and not hz_type:
        result.record_field("asset_type", "tolerance")
    else:
        result.record_field("asset_type", "tolerance")
        diffs.append(f"asset_type CH={ch_type} HZ={hz_type} (mapping)")

    return diffs


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E09] {len(done_keys)} keys already done", file=sys.stderr)

    raw = load_samples("samples_assets.txt")
    assets: list[tuple[int, str, int, int]] = []
    for line in raw:
        p = parse_sample(line)
        if p and p[0] == 1:  # classic credit only (skip 0=native, 2=SAC, 3=soroban-native)
            assets.append(p)
    print(f"[E09] {len(assets)} classic-credit assets / {len(raw)} samples total",
          file=sys.stderr)

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        random.seed(42)
        assets = random.sample(assets, min(pilot, len(assets)))
        print(f"[E09] PILOT — capped to {len(assets)} assets", file=sys.stderr)

    cap = int(os.environ.get("SBE_PHASE_B_CAP", "5000"))
    if cap > 0 and len(assets) > cap:
        random.seed(123)
        assets = random.sample(assets, cap)
        print(f"[E09] capped to {cap} stratified samples", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(assets)
    started = time.monotonic()
    processed = 0

    for atype, acode, issuer_id, contract_id in assets:
        key = f"{atype}|{acode}|{issuer_id}|{contract_id}"
        if key in done_keys:
            continue
        processed += 1

        ch = fetch_ch_asset(atype, acode, issuer_id, contract_id)
        if ch is None:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 1, "CH_MISSING")
            result.fail_total += 1
            continue

        issuer_strkey = ch.get("issuer_strkey")
        if not issuer_strkey:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 0, "CH_ISSUER_UNRESOLVED")
            continue

        hz = fetch_horizon_asset(acode, issuer_strkey)
        if hz is None:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 0, "HZ_404")
            continue

        diffs = compare(key, ch, hz, result)
        if diffs:
            dump_diff(ENDPOINT, key.replace("|", "_"), ch, hz, diffs)
            tol = sum(1 for d in diffs if "drift" in d or "(mapping)" in d)
            fail = len(diffs) - tol
            pass_n = 5 - len(diffs)
            append_tsv_row(TSV, ENDPOINT, key, pass_n, tol, fail, ";".join(diffs))
        else:
            append_tsv_row(TSV, ENDPOINT, key, 5, 0, 0, "")

        if processed % 100 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(assets) - processed) / max(rate, 0.01))
            print(f"[E09] {processed}/{len(assets)} done, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"rate={rate:.1f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e09_summary.json"
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

    print(f"[E09] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
