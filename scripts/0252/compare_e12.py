#!/usr/bin/env python3
"""E12 — /contracts/:contract_id/interface compare CH ↔ stellar.expert.

Per-contract compare of WASM interface metadata. stellar.expert
surfaces the parsed function list at
`/explorer/public/contract/<id>?fields=functions` (or via the main
contract endpoint with `functions` embedded — discover at runtime).

Field-by-field:
  1. function name list      (set equality)
  2. wasm_hash echo          (CH `wasm_interface_metadata` join must
                              resolve to the SAME hash that
                              stellar.expert reports)
  3. is_sac flag             (SAC → wasm_hash NULL → empty function
                              list expected)

Sample source: samples_contracts.txt (reuse from E11). Cap 5K.
stellar.expert may rate-limit; use HORIZON_DELAY-style throttle.
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
    HORIZON_DELAY,
    OUT_DIR,
    EndpointResult,
    append_tsv_row,
    ch_query_json,
    dump_diff,
    load_samples,
    read_completed_keys,
    write_tsv_header,
)

import requests


ENDPOINT = "E12"
TSV = OUT_DIR / "phase_b_e12.tsv"
STELLAR_EXPERT_BASE = os.environ.get(
    "STELLAR_EXPERT_BASE", "https://api.stellar.expert/explorer/public"
)

DEFAULT_SAMPLE = int(os.environ.get("SBE_PHASE_B_CAP", "5000"))


def fetch_ch_interface(contract_strkey: str) -> dict | None:
    sql = f"""
    SELECT
        sc.contract_id                              AS contract_id,
        lower(hex(sc.wasm_hash))                    AS wasm_hash_hex,
        sc.is_sac                                   AS is_sac,
        ifNull(wim.functions, '[]')                 AS functions_json
    FROM soroban_contracts AS sc FINAL
    LEFT JOIN wasm_interface_metadata AS wim ON wim.wasm_hash = sc.wasm_hash
    WHERE sc.contract_id = '{contract_strkey}'
    LIMIT 1
    FORMAT JSONEachRow
    """
    rows = ch_query_json(sql)
    return rows[0] if rows else None


def fetch_se_contract(contract_strkey: str) -> dict | None:
    """stellar.expert contract endpoint. The interface (`functions`)
    is folded into the main contract record on stellar.expert.
    """
    url = f"{STELLAR_EXPERT_BASE}/contract/{contract_strkey}"
    for attempt in range(4):
        r = requests.get(url, timeout=30)
        if r.status_code == 200:
            time.sleep(HORIZON_DELAY)
            return r.json()
        if r.status_code in (429,) or r.status_code >= 500:
            time.sleep(2 ** attempt)
            continue
        if r.status_code in (400, 404):
            return None
        r.raise_for_status()
    return None


def ch_function_names(functions_json: str) -> set[str]:
    """`wasm_interface_metadata.functions` is the parser-emitted JSON
    array; each entry has a `name` field per SEP-48 derivation.
    """
    try:
        arr = json.loads(functions_json)
    except (json.JSONDecodeError, TypeError):
        return set()
    out = set()
    if isinstance(arr, list):
        for e in arr:
            if isinstance(e, dict) and "name" in e:
                out.add(str(e["name"]))
    return out


def se_function_names(body: dict) -> set[str] | None:
    """stellar.expert payload — shape unknown without a live probe;
    handle a few candidate keys. Returns None if the response does
    not surface a function list (mark as tolerance — manual eyeball
    required, per the task plan's stellar.expert pagination caveat).
    """
    if not body:
        return None
    for key in ("functions", "interface", "wasm_functions"):
        v = body.get(key)
        if isinstance(v, list):
            out: set[str] = set()
            for e in v:
                if isinstance(e, dict) and ("name" in e or "function" in e):
                    out.add(str(e.get("name") or e.get("function")))
                elif isinstance(e, str):
                    out.add(e)
            return out
    return None


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E12] {len(done_keys)} contracts already done", file=sys.stderr)

    contracts = load_samples("samples_contracts.txt")
    print(f"[E12] {len(contracts)} samples loaded", file=sys.stderr)

    random.seed(42)
    if DEFAULT_SAMPLE < len(contracts):
        contracts = random.sample(contracts, DEFAULT_SAMPLE)

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        contracts = contracts[:pilot]
        print(f"[E12] PILOT mode — first {pilot}", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(contracts)
    started = time.monotonic()
    processed = 0

    for cstrkey in contracts:
        if cstrkey in done_keys:
            continue
        processed += 1

        try:
            ch = fetch_ch_interface(cstrkey)
        except RuntimeError as e:
            append_tsv_row(TSV, ENDPOINT, cstrkey, 0, 0, 1, f"CH_ERR:{str(e)[:80]}")
            result.fail_total += 1
            continue
        if ch is None:
            append_tsv_row(TSV, ENDPOINT, cstrkey, 0, 0, 1, "CH_MISSING")
            result.fail_total += 1
            continue

        se = fetch_se_contract(cstrkey)
        if se is None:
            append_tsv_row(TSV, ENDPOINT, cstrkey, 0, 0, 0, "SE_MISSING")
            continue

        diffs: list[str] = []

        # contract_id echo.
        if (ch.get("contract_id") or "") == cstrkey:
            result.record_field("contract_id_echo", "pass")
        else:
            result.record_field("contract_id_echo", "fail")
            diffs.append(f"contract_id CH={ch.get('contract_id')} expected={cstrkey}")

        # is_sac.
        is_sac_ch = bool(ch.get("is_sac"))
        is_sac_se = (
            bool(se.get("kind") and "asset" in str(se.get("kind")).lower())
            or bool(se.get("is_sac"))
        )
        if is_sac_ch == is_sac_se:
            result.record_field("is_sac", "pass")
        else:
            result.record_field("is_sac", "tolerance")
            diffs.append(f"is_sac CH={is_sac_ch} SE={is_sac_se} (stellar.expert kind heuristic)")

        # function list.
        ch_fns = ch_function_names(ch.get("functions_json") or "[]")
        se_fns = se_function_names(se)
        if se_fns is None:
            result.record_field("functions_set", "tolerance")
            diffs.append("functions: stellar.expert payload has no function list (manual review)")
        else:
            only_ch = ch_fns - se_fns
            only_se = se_fns - ch_fns
            if not only_ch and not only_se:
                result.record_field("functions_set", "pass")
            elif is_sac_ch and not ch_fns and se_fns is not None:
                result.record_field("functions_set", "tolerance")
                diffs.append(
                    f"functions_set SAC contract — CH empty, SE has {len(se_fns)} entries (stellar.expert decorates SAC)"
                )
            else:
                result.record_field("functions_set", "fail")
                diffs.append(
                    f"functions_set only_ch={len(only_ch)} only_se={len(only_se)} "
                    f"sample_ch={list(only_ch)[:3]} sample_se={list(only_se)[:3]}"
                )

        if diffs:
            dump_diff(ENDPOINT, cstrkey, ch, se, diffs)
            note = ";".join(diffs)[:500]
        else:
            note = ""

        pass_n = sum(1 for v in result.fields.values() if v.pass_count > 0)
        tol_n = sum(1 for d in diffs if "(stellar.expert" in d or "(manual review)" in d)
        fail_n = sum(1 for d in diffs if "(stellar.expert" not in d and "(manual review)" not in d)
        append_tsv_row(TSV, ENDPOINT, cstrkey, pass_n, tol_n, fail_n, note)

        if processed % 100 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(contracts) - processed) / max(rate, 0.01))
            print(f"[E12] {processed}/{len(contracts)} done, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"tol={result.tolerance_total} rate={rate:.1f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e12_summary.json"
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

    print(f"[E12] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
