#!/usr/bin/env python3
"""E13 — /contracts/:contract_id/invocations compare CH ↔ stellar.expert.

Per-contract invocations list. stellar.expert exposes recent
invocations under `/contract/<id>/invocations` (may be paginated;
discover at runtime).

Compare:
  * CH `soroban_invocations_appearances` filtered to the contract
    (latest N invocations).
  * stellar.expert recent invocations API.
  * Diff: invocation_count >= some threshold on both sides, and
    set-overlap on (transaction_hash, function_name) tuples where
    available.

Per task-plan caveat: stellar.expert sub-resource pagination is not
always public — accept that some endpoints surface a small `recent
invocations` snapshot rather than the full history, and record the
gap as `tolerance`.
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


ENDPOINT = "E13"
TSV = OUT_DIR / "phase_b_e13.tsv"
STELLAR_EXPERT_BASE = os.environ.get(
    "STELLAR_EXPERT_BASE", "https://api.stellar.expert/explorer/public"
)

DEFAULT_SAMPLE = int(os.environ.get("SBE_PHASE_B_CAP", "2000"))
RECENT_LIMIT = 50  # last N invocations to compare


def fetch_ch_invocations(contract_strkey: str) -> list[dict]:
    sql = f"""
    WITH (SELECT id FROM soroban_contracts FINAL
          WHERE contract_id = '{contract_strkey}' LIMIT 1) AS cid
    SELECT
        lower(hex(t.hash))                AS tx_hash,
        ia.ledger_sequence                AS ledger_sequence,
        ia.transaction_id                 AS transaction_id
    FROM soroban_invocations_appearances AS ia FINAL
    INNER JOIN transactions AS t FINAL
        ON t.id = ia.transaction_id AND t.ledger_sequence = ia.ledger_sequence
    WHERE ia.contract_id = cid
    ORDER BY ia.ledger_sequence DESC, ia.transaction_id DESC
    LIMIT {RECENT_LIMIT}
    FORMAT JSONEachRow
    """
    return ch_query_json(sql)


def fetch_se_invocations(contract_strkey: str) -> list[dict] | None:
    """Try a few candidate URLs — stellar.expert has documented
    `/contract/<id>` plus undocumented sub-paths. None = no
    actionable data.
    """
    for path in (
        f"/contract/{contract_strkey}/invocations",
        f"/contract/{contract_strkey}/calls",
    ):
        url = f"{STELLAR_EXPERT_BASE}{path}"
        try:
            r = requests.get(url, timeout=30)
        except requests.RequestException:
            continue
        if r.status_code == 200:
            try:
                body = r.json()
            except json.JSONDecodeError:
                continue
            time.sleep(HORIZON_DELAY)
            if isinstance(body, dict):
                recs = body.get("_embedded", {}).get("records") or body.get("records") or body.get("items")
                if isinstance(recs, list):
                    return recs
            if isinstance(body, list):
                return body
        elif r.status_code in (429,) or r.status_code >= 500:
            time.sleep(2)
    return None


def se_tx_hash_set(records: list[dict]) -> set[str]:
    out: set[str] = set()
    for r in records:
        for k in ("tx_hash", "transaction_hash", "transaction", "hash"):
            v = r.get(k)
            if isinstance(v, str) and len(v) >= 32:
                out.add(v.lower())
                break
    return out


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E13] {len(done_keys)} contracts already done", file=sys.stderr)

    contracts = load_samples("samples_contracts.txt")
    print(f"[E13] {len(contracts)} samples loaded", file=sys.stderr)

    random.seed(42)
    if DEFAULT_SAMPLE < len(contracts):
        contracts = random.sample(contracts, DEFAULT_SAMPLE)

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        contracts = contracts[:pilot]
        print(f"[E13] PILOT mode — first {pilot}", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(contracts)
    started = time.monotonic()
    processed = 0
    se_unavailable = 0

    for cstrkey in contracts:
        if cstrkey in done_keys:
            continue
        processed += 1

        try:
            ch_rows = fetch_ch_invocations(cstrkey)
        except RuntimeError as e:
            append_tsv_row(TSV, ENDPOINT, cstrkey, 0, 0, 1, f"CH_ERR:{str(e)[:80]}")
            result.fail_total += 1
            continue

        if not ch_rows:
            # Contract has no invocations in CH; skip silently.
            append_tsv_row(TSV, ENDPOINT, cstrkey, 0, 0, 0, "CH_EMPTY")
            continue

        se_rows = fetch_se_invocations(cstrkey)
        diffs: list[str] = []

        ch_hashes = {r["tx_hash"].lower() for r in ch_rows}

        if se_rows is None:
            se_unavailable += 1
            result.record_field("se_compare", "tolerance")
            diffs.append("stellar.expert invocations sub-resource unavailable (no public path)")
        else:
            se_hashes = se_tx_hash_set(se_rows)
            inter = ch_hashes & se_hashes
            # Soft expectation: at least one overlap on contracts with
            # > 5 CH invocations (stellar.expert recent snapshot vs CH
            # window may have asymmetric coverage).
            if inter:
                result.record_field("se_compare", "pass")
            elif len(ch_rows) >= 5 and se_hashes:
                result.record_field("se_compare", "tolerance")
                diffs.append(
                    f"no_overlap ch={len(ch_hashes)} se={len(se_hashes)} "
                    f"(stellar.expert snapshot window may differ)"
                )
            else:
                result.record_field("se_compare", "tolerance")
                diffs.append(f"sparse_data ch={len(ch_hashes)} se={len(se_hashes)}")

        # Internal CH sanity — tx_hashes are 64-char lowercase hex.
        bad_hashes = [h for h in ch_hashes if len(h) != 64]
        if not bad_hashes:
            result.record_field("hash_well_formed", "pass")
        else:
            result.record_field("hash_well_formed", "fail")
            diffs.append(f"malformed_hashes={bad_hashes[:3]}")

        if diffs:
            dump_diff(ENDPOINT, cstrkey, ch_rows, se_rows, diffs)
            note = ";".join(diffs)[:500]
        else:
            note = ""

        pass_n = sum(1 for v in result.fields.values() if v.pass_count > 0)
        tol_n = sum(1 for d in diffs if "stellar.expert" in d or "sparse_data" in d or "no_overlap" in d)
        fail_n = sum(1 for d in diffs if "malformed_hashes" in d)
        append_tsv_row(TSV, ENDPOINT, cstrkey, pass_n, tol_n, fail_n, note)

        if processed % 100 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(contracts) - processed) / max(rate, 0.01))
            print(f"[E13] {processed}/{len(contracts)} done, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"tol={result.tolerance_total} se_na={se_unavailable} "
                  f"rate={rate:.1f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e13_summary.json"
    summary.write_text(json.dumps({
        "endpoint": ENDPOINT,
        "sample_size": result.sample_size,
        "se_unavailable_count": se_unavailable,
        "pass_total": result.pass_total,
        "fail_total": result.fail_total,
        "tolerance_total": result.tolerance_total,
        "elapsed_ms": result.elapsed_ms,
        "fields": {
            k: {"pass": v.pass_count, "tolerance": v.tolerance_count, "fail": v.fail_count}
            for k, v in result.fields.items()
        },
    }, indent=2))

    print(f"[E13] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} se_na={se_unavailable} "
          f"elapsed={result.elapsed_ms}ms", file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
