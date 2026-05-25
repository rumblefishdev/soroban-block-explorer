#!/usr/bin/env python3
"""E10 — /assets/:id/transactions internal sanity.

Sample N assets (mixed types from `assets FINAL`), and for each:
  * Run the canonical filter against `operations_appearances` for that
    asset's identity (classic credit OR contract).
  * Walk 5 pages × 50 rows.
  * Assert:
    - `transaction_id` FK resolves into `transactions FINAL`
    - cursor (`ledger_sequence DESC, transaction_id DESC`) monotonic.

`operations_appearances.contract_id` for non-classic assets — taken
directly from `assets.contract_id`. For classic credits the filter is
`(asset_code, asset_issuer_id)`.
"""

from __future__ import annotations

import json
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
    write_tsv_header,
)
from phase_d_common import assert_monotonic_desc_loose, ch_scalar


ENDPOINT = "E10"
TSV = OUT_DIR / "phase_d_e10.tsv"

SAMPLE = 200
PAGES = 5
LIMIT = 50


def sample_assets(n: int) -> list[dict]:
    sql = f"""
    SELECT asset_type, asset_code, issuer_id, contract_id
    FROM assets FINAL
    WHERE issuer_id != 0 OR contract_id != 0
    ORDER BY cityHash64(asset_code, issuer_id, contract_id)
    LIMIT {n}
    FORMAT JSONEachRow
    """
    return ch_query_json(sql)


def walk_tx(asset: dict) -> tuple[list[int], list[int]]:
    """Return (ledger_seqs, tx_ids) walked across PAGES pages for `asset`."""
    ac = asset.get("asset_code") or ""
    iid = int(asset.get("issuer_id") or 0)
    cid = int(asset.get("contract_id") or 0)

    if cid != 0:
        filter_sql = f"oa.contract_id = {cid}"
    else:
        ac_esc = ac.replace("'", "''")
        filter_sql = f"oa.asset_code = '{ac_esc}' AND oa.asset_issuer_id = {iid}"

    ledger_seqs: list[int] = []
    tx_ids: list[int] = []
    cursor: tuple[int, int] | None = None

    for _ in range(PAGES):
        cursor_pred = (
            f"AND (oa.ledger_sequence, oa.transaction_id) < ({cursor[0]}, {cursor[1]})"
            if cursor
            else ""
        )
        sql = f"""
        SELECT DISTINCT
            oa.ledger_sequence    AS ledger_sequence,
            oa.transaction_id     AS transaction_id
        FROM operations_appearances AS oa
        WHERE {filter_sql}
        {cursor_pred}
        ORDER BY oa.ledger_sequence DESC, oa.transaction_id DESC
        LIMIT {LIMIT}
        FORMAT JSONEachRow
        """
        rows = ch_query_json(sql)
        if not rows:
            break
        for r in rows:
            ledger_seqs.append(int(r["ledger_sequence"]))
            tx_ids.append(int(r["transaction_id"]))
        last = rows[-1]
        cursor = (int(last["ledger_sequence"]), int(last["transaction_id"]))

    return ledger_seqs, tx_ids


def main() -> int:
    write_tsv_header(TSV)
    result = EndpointResult(endpoint=ENDPOINT)
    started = time.monotonic()
    random.seed(42)

    assets = sample_assets(SAMPLE)
    print(f"[E10] {len(assets)} asset samples", file=sys.stderr)
    result.sample_size = len(assets)

    cursor_fail = 0
    fk_fail = 0
    walked = 0
    processed = 0
    diffs_collected: list[str] = []

    for asset in assets:
        processed += 1
        try:
            ledger_seqs, tx_ids = walk_tx(asset)
        except RuntimeError as e:
            result.record_field("walk", "fail")
            diffs_collected.append(f"walk_error asset={asset}: {str(e)[:120]}")
            continue

        if not tx_ids:
            result.record_field("walk", "pass")  # vacuously
            continue
        walked += len(tx_ids)

        # Monotonic check on tuple cursor.
        tuples = list(zip(ledger_seqs, tx_ids))
        if assert_monotonic_desc_loose(tuples):
            result.record_field("cursor_monotonic", "pass")
        else:
            result.record_field("cursor_monotonic", "fail")
            cursor_fail += 1
            diffs_collected.append(f"cursor not monotonic for asset={asset.get('contract_id')}")

        # FK resolve — check chunk of unique tx ids.
        uniq = list(set(tx_ids))[:200]
        ids_csv = ",".join(str(t) for t in uniq)
        present = int(
            ch_scalar(
                f"SELECT count(DISTINCT id) FROM transactions FINAL "
                f"WHERE id IN ({ids_csv}) FORMAT TabSeparated"
            )
            or 0
        )
        if present == len(uniq):
            result.record_field("tx_fk", "pass")
        else:
            result.record_field("tx_fk", "fail")
            fk_fail += 1
            diffs_collected.append(
                f"tx_fk: {present}/{len(uniq)} resolved for asset={asset.get('contract_id')}"
            )

        if processed % 50 == 0:
            print(f"[E10] {processed}/{len(assets)} walked", file=sys.stderr)

    pass_n = sum(v.pass_count for v in result.fields.values())
    fail_n = sum(v.fail_count for v in result.fields.values())

    if diffs_collected:
        dump_diff(ENDPOINT, "walk", {"sample": len(assets), "rows": walked}, None, diffs_collected[:50])

    append_tsv_row(TSV, ENDPOINT, "walk", pass_n, 0, fail_n,
                   f"assets={len(assets)} rows={walked} cursor_fail={cursor_fail} fk_fail={fk_fail}")
    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_d_e10_summary.json"
    summary.write_text(json.dumps({
        "endpoint": ENDPOINT,
        "sample_size": result.sample_size,
        "rows_walked": walked,
        "pass_total": result.pass_total,
        "fail_total": result.fail_total,
        "tolerance_total": result.tolerance_total,
        "elapsed_ms": result.elapsed_ms,
        "fields": {
            k: {"pass": v.pass_count, "tolerance": v.tolerance_count, "fail": v.fail_count}
            for k, v in result.fields.items()
        },
    }, indent=2))

    print(f"[E10] done: assets={len(assets)} rows={walked} "
          f"pass={result.pass_total} fail={result.fail_total} "
          f"elapsed={result.elapsed_ms}ms", file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
