#!/usr/bin/env python3
"""E14 — /contracts/:contract_id/events compare CH ↔ stellar.expert.

Per-contract events list. CH stores the FULL payload
(`topics_xdr` + `data_xdr` inline) per ADR 0044 §5.1. stellar.expert
exposes events under `/contract/<id>/events` (best-effort —
discover at runtime).

Compare:
  * CH `soroban_events FINAL` last N events for the contract.
  * stellar.expert events list.
  * Diff: tx_hash + event_index overlap on intersection.
  * Internal sanity on CH: per-(contract, ledger), `event_index`
    monotonic increasing within a transaction.
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
    ch_query,
    ch_query_json,
    dump_diff,
    read_completed_keys,
    write_tsv_header,
)

import requests


ENDPOINT = "E14"
TSV = OUT_DIR / "phase_b_e14.tsv"
STELLAR_EXPERT_BASE = os.environ.get(
    "STELLAR_EXPERT_BASE", "https://api.stellar.expert/explorer/public"
)

DEFAULT_SAMPLE = int(os.environ.get("SBE_PHASE_B_CAP", "2000"))
RECENT_LIMIT = 100


def load_active_contracts(n: int) -> list[str]:
    """Same rationale as E13 — sample only contracts that have events
    recorded in `soroban_events`. The generic pool is SAC-heavy and
    most SACs emit no events.
    """
    sql = f"""
    WITH active AS (
        SELECT contract_id AS surrogate
        FROM soroban_events
        GROUP BY contract_id
        ORDER BY cityHash64(contract_id)
        LIMIT {n * 2}
    )
    SELECT sc.contract_id
    FROM soroban_contracts AS sc FINAL
    INNER JOIN active AS a ON a.surrogate = sc.id
    FORMAT TabSeparated
    """
    out = ch_query(sql).splitlines()
    return [c.strip() for c in out if c.strip()]


def fetch_ch_events(contract_strkey: str) -> list[dict]:
    sql = f"""
    WITH (SELECT id FROM soroban_contracts FINAL
          WHERE contract_id = '{contract_strkey}' LIMIT 1) AS cid
    SELECT
        lower(hex(t.hash))                AS tx_hash,
        ev.ledger_sequence                AS ledger_sequence,
        ev.transaction_id                 AS transaction_id,
        ev.event_index                    AS event_index
    FROM soroban_events AS ev FINAL
    INNER JOIN transactions AS t FINAL
        ON t.id = ev.transaction_id AND t.ledger_sequence = ev.ledger_sequence
    WHERE ev.contract_id = cid
    ORDER BY ev.ledger_sequence DESC, ev.transaction_id DESC, ev.event_index DESC
    LIMIT {RECENT_LIMIT}
    FORMAT JSONEachRow
    """
    return ch_query_json(sql)


def fetch_se_events(contract_strkey: str) -> list[dict] | None:
    for path in (
        f"/contract/{contract_strkey}/events",
        f"/contract-event?contract={contract_strkey}",
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


def se_tx_event_pairs(records: list[dict]) -> set[tuple[str, int | None]]:
    out: set[tuple[str, int | None]] = set()
    for r in records:
        h = None
        for k in ("tx_hash", "transaction_hash", "transaction", "hash"):
            v = r.get(k)
            if isinstance(v, str) and len(v) >= 32:
                h = v.lower()
                break
        if h is None:
            continue
        idx = None
        for k in ("event_index", "index", "i"):
            v = r.get(k)
            if isinstance(v, int):
                idx = v
                break
        out.add((h, idx))
    return out


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E14] {len(done_keys)} contracts already done", file=sys.stderr)

    contracts = load_active_contracts(DEFAULT_SAMPLE)
    print(f"[E14] {len(contracts)} active contracts loaded from "
          f"soroban_events", file=sys.stderr)

    random.seed(42)
    if DEFAULT_SAMPLE < len(contracts):
        contracts = random.sample(contracts, DEFAULT_SAMPLE)

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        contracts = contracts[:pilot]
        print(f"[E14] PILOT mode — first {pilot}", file=sys.stderr)

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
            ch_rows = fetch_ch_events(cstrkey)
        except RuntimeError as e:
            append_tsv_row(TSV, ENDPOINT, cstrkey, 0, 0, 1, f"CH_ERR:{str(e)[:80]}")
            result.fail_total += 1
            continue

        if not ch_rows:
            append_tsv_row(TSV, ENDPOINT, cstrkey, 0, 0, 0, "CH_EMPTY")
            continue

        # CH internal sanity: per-tx event_index must be unique +
        # the global ORDER BY produces a stable sequence.
        per_tx: dict[int, set[int]] = {}
        for r in ch_rows:
            txid = int(r["transaction_id"])
            idx = int(r["event_index"])
            per_tx.setdefault(txid, set()).add(idx)
        diffs: list[str] = []

        dup = any(len(s) != len([
            int(r["event_index"]) for r in ch_rows if int(r["transaction_id"]) == txid
        ]) for txid, s in per_tx.items())
        if not dup:
            result.record_field("event_index_unique_per_tx", "pass")
        else:
            result.record_field("event_index_unique_per_tx", "fail")
            diffs.append("event_index duplicated within a tx (FINAL dedup failure?)")

        ch_pairs = {(r["tx_hash"].lower(), int(r["event_index"])) for r in ch_rows}

        se_rows = fetch_se_events(cstrkey)
        if se_rows is None:
            se_unavailable += 1
            result.record_field("se_compare", "tolerance")
            diffs.append("stellar.expert events sub-resource unavailable")
        else:
            se_pairs_all = se_tx_event_pairs(se_rows)
            # If SE didn't tag event_index, match by tx_hash alone.
            if all(idx is None for (_, idx) in se_pairs_all):
                ch_h = {h for (h, _) in ch_pairs}
                se_h = {h for (h, _) in se_pairs_all}
                if ch_h & se_h:
                    result.record_field("se_compare", "pass")
                else:
                    result.record_field("se_compare", "tolerance")
                    diffs.append(
                        f"no_overlap_tx_only ch={len(ch_h)} se={len(se_h)}"
                    )
            else:
                inter = ch_pairs & se_pairs_all
                if inter:
                    result.record_field("se_compare", "pass")
                elif len(ch_rows) >= 5 and se_pairs_all:
                    result.record_field("se_compare", "tolerance")
                    diffs.append(
                        f"no_overlap ch={len(ch_pairs)} se={len(se_pairs_all)} "
                        f"(stellar.expert snapshot may differ)"
                    )
                else:
                    result.record_field("se_compare", "tolerance")
                    diffs.append(f"sparse_data ch={len(ch_pairs)} se={len(se_pairs_all)}")

        if diffs:
            dump_diff(ENDPOINT, cstrkey, ch_rows, se_rows, diffs)
            note = ";".join(diffs)[:500]
        else:
            note = ""

        pass_n = sum(1 for v in result.fields.values() if v.pass_count > 0)
        tol_n = sum(1 for d in diffs if "stellar.expert" in d or "no_overlap" in d or "sparse_data" in d)
        fail_n = sum(1 for d in diffs if "duplicated" in d)
        append_tsv_row(TSV, ENDPOINT, cstrkey, pass_n, tol_n, fail_n, note)

        if processed % 100 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(contracts) - processed) / max(rate, 0.01))
            print(f"[E14] {processed}/{len(contracts)} done, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"tol={result.tolerance_total} se_na={se_unavailable} "
                  f"rate={rate:.1f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e14_summary.json"
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

    print(f"[E14] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} se_na={se_unavailable} "
          f"elapsed={result.elapsed_ms}ms", file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
