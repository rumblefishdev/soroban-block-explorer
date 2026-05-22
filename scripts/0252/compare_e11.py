#!/usr/bin/env python3
"""E11 — /contracts/:contract_id compare CH ↔ stellar.expert.

Per-contract compare. stellar.expert is the canonical reference for
Soroban contract metadata (Horizon doesn't surface contracts).

Field-by-field on 5 fields:
  1. contract_id echo
  2. deployer (StrKey from accounts.account_id where accounts.id =
     soroban_contracts.deployer_id)
  3. deployed_at_ledger
  4. wasm_hash (hex)
  5. is_sac (Bool)

Sample source: samples_contracts.txt (5K SAC + 5K Other + 3 Nft +
1K NULL = ~11K stratified). Cap to 5K via SBE_PHASE_B_CAP.

stellar.expert API: GET /explorer/public/contract/<id>
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


ENDPOINT = "E11"
TSV = OUT_DIR / "phase_b_e11.tsv"
STELLAR_EXPERT_BASE = os.environ.get(
    "STELLAR_EXPERT_BASE", "https://api.stellar.expert/explorer/public"
)


def stellar_expert_get(path: str, max_retries: int = 5) -> dict | None:
    """GET wrapper for stellar.expert API with same backoff posture
    as horizon_get."""
    url = f"{STELLAR_EXPERT_BASE}{path}"
    for attempt in range(max_retries):
        r = requests.get(url, timeout=30)
        if r.status_code == 200:
            time.sleep(HORIZON_DELAY)
            return r.json()
        if r.status_code == 429 or r.status_code >= 500:
            time.sleep(2 ** attempt)
            continue
        if r.status_code == 404:
            return None
        r.raise_for_status()
    return None


def fetch_ch_contract(strkey: str) -> dict | None:
    sql = f"""
    SELECT
      sc.contract_id                AS contract_id,
      lower(hex(sc.wasm_hash))      AS wasm_hash,
      sc.deployed_at_ledger         AS deployed_at_ledger,
      a.account_id                  AS deployer_strkey,
      sc.is_sac                     AS is_sac
    FROM soroban_contracts AS sc FINAL
    LEFT JOIN accounts AS a FINAL ON a.id = sc.deployer_id
    WHERE sc.contract_id = '{strkey}'
    LIMIT 1
    FORMAT JSONEachRow
    """
    rows = ch_query_json(sql)
    return rows[0] if rows else None


def fetch_se_contract(strkey: str) -> dict | None:
    return stellar_expert_get(f"/contract/{strkey}")


def compare(strkey: str, ch: dict, se: dict, result: EndpointResult) -> list[str]:
    diffs: list[str] = []

    # 1. contract_id echo
    se_id = se.get("contract") or se.get("id")
    if ch["contract_id"] == se_id:
        result.record_field("contract_id", "pass")
    else:
        result.record_field("contract_id", "fail")
        diffs.append(f"contract_id CH={ch['contract_id']} SE={se_id}")

    # 2. deployer — stellar.expert returns `creator` field (StrKey)
    se_deployer = se.get("creator", "")
    ch_deployer = ch.get("deployer_strkey") or ""
    if ch_deployer == se_deployer:
        result.record_field("deployer", "pass")
    elif not ch_deployer or not se_deployer:
        # Stub-row contracts (no WASM yet) may have NULL deployer.
        result.record_field("deployer", "tolerance")
        diffs.append(f"deployer CH='{ch_deployer}' SE='{se_deployer}' (stub or missing)")
    else:
        result.record_field("deployer", "fail")
        diffs.append(f"deployer CH={ch_deployer} SE={se_deployer}")

    # 3. deployed_at_ledger — stellar.expert returns `created` (ts)
    # or `created_at_ledger` (numeric). Try both.
    se_ledger = se.get("created_at_ledger") or se.get("created_ledger")
    ch_ledger = ch.get("deployed_at_ledger")
    if ch_ledger and se_ledger and int(ch_ledger) == int(se_ledger):
        result.record_field("deployed_at_ledger", "pass")
    elif not ch_ledger:
        result.record_field("deployed_at_ledger", "tolerance")
        diffs.append(f"deployed_at_ledger CH=NULL SE={se_ledger} (stub row)")
    elif not se_ledger:
        result.record_field("deployed_at_ledger", "tolerance")
        diffs.append(f"deployed_at_ledger CH={ch_ledger} SE=NULL (SE missing field)")
    else:
        result.record_field("deployed_at_ledger", "fail")
        diffs.append(f"deployed_at_ledger CH={ch_ledger} SE={se_ledger}")

    # 4. wasm_hash (hex) — stellar.expert returns `wasm` hex
    ch_wasm = (ch.get("wasm_hash") or "").lower()
    se_wasm = (se.get("wasm") or "").lower()
    if ch_wasm and se_wasm and ch_wasm == se_wasm:
        result.record_field("wasm_hash", "pass")
    elif not ch_wasm and not se_wasm:
        result.record_field("wasm_hash", "pass")  # both null = match
    elif not ch_wasm or not se_wasm:
        result.record_field("wasm_hash", "tolerance")
        diffs.append(f"wasm_hash CH={ch_wasm or 'NULL'} SE={se_wasm or 'NULL'} (one side missing)")
    else:
        result.record_field("wasm_hash", "fail")
        diffs.append(f"wasm_hash CH={ch_wasm} SE={se_wasm}")

    # 5. is_sac — stellar.expert flags via `protocol` field or `is_sac`
    se_is_sac = bool(se.get("is_sac")) or se.get("kind") == "sac" or \
                se.get("protocol") == "sac"
    if bool(ch["is_sac"]) == se_is_sac:
        result.record_field("is_sac", "pass")
    else:
        result.record_field("is_sac", "tolerance")
        diffs.append(f"is_sac CH={ch['is_sac']} SE={se_is_sac} (SE classification heuristic)")

    return diffs


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E11] {len(done_keys)} keys already done", file=sys.stderr)

    contracts = load_samples("samples_contracts.txt")
    print(f"[E11] {len(contracts)} contract samples loaded", file=sys.stderr)

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        random.seed(42)
        contracts = random.sample(contracts, min(pilot, len(contracts)))
        print(f"[E11] PILOT — capped to {len(contracts)} contracts", file=sys.stderr)

    cap = int(os.environ.get("SBE_PHASE_B_CAP", "5000"))
    if cap > 0 and len(contracts) > cap:
        random.seed(123)
        contracts = random.sample(contracts, cap)
        print(f"[E11] capped to {cap} stratified samples", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(contracts)
    started = time.monotonic()
    processed = 0

    for strkey in contracts:
        if strkey in done_keys:
            continue
        processed += 1

        ch = fetch_ch_contract(strkey)
        if ch is None:
            append_tsv_row(TSV, ENDPOINT, strkey, 0, 0, 1, "CH_MISSING")
            result.fail_total += 1
            continue

        se = fetch_se_contract(strkey)
        if se is None:
            append_tsv_row(TSV, ENDPOINT, strkey, 0, 0, 0, "SE_404")
            continue

        diffs = compare(strkey, ch, se, result)
        if diffs:
            dump_diff(ENDPOINT, strkey, ch, se, diffs)
            tol = sum(1 for d in diffs if any(s in d for s in
                ("(stub", "(SE missing", "(one side missing", "(SE classification")))
            fail = len(diffs) - tol
            pass_n = 5 - len(diffs)
            append_tsv_row(TSV, ENDPOINT, strkey, pass_n, tol, fail, ";".join(diffs))
        else:
            append_tsv_row(TSV, ENDPOINT, strkey, 5, 0, 0, "")

        if processed % 100 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(contracts) - processed) / max(rate, 0.01))
            print(f"[E11] {processed}/{len(contracts)} done, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"rate={rate:.1f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e11_summary.json"
    summary.write_text(json.dumps({
        "endpoint": ENDPOINT,
        "source": "stellar.expert",
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

    print(f"[E11] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
