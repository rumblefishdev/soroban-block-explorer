#!/usr/bin/env python3
"""E08 — /assets list internal sanity.

Paginated walk via the natural 4-tuple keyset cursor
`(asset_type DESC, asset_code DESC, issuer_id DESC, contract_id DESC)`.
Per page checks:
  * tuple cursor strictly monotonic DESC (no duplicates, no inversions)
  * for every non-zero `issuer_id`, the issuer resolves to a row in
    `accounts FINAL`
  * for every non-zero `contract_id`, the contract resolves in
    `soroban_contracts FINAL`

Walks until 30 pages × 50 = 1500 rows or page count exhausted.
"""

from __future__ import annotations

import json
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
from phase_d_common import assert_monotonic_desc_loose


ENDPOINT = "E08"
TSV = OUT_DIR / "phase_d_e08.tsv"

PAGES = 30
LIMIT = 50


def fetch_page(cursor: tuple | None) -> list[dict]:
    if cursor is None:
        where_cursor = "1=1"
    else:
        at, ac, iid, cid = cursor
        ac_esc = ac.replace("'", "''")
        where_cursor = (
            f"(asset_type, asset_code, issuer_id, contract_id) < "
            f"({at}, '{ac_esc}', {iid}, {cid})"
        )
    sql = f"""
    SELECT
        asset_type,
        asset_code,
        issuer_id,
        contract_id
    FROM assets FINAL
    WHERE {where_cursor}
    ORDER BY asset_type DESC, asset_code DESC, issuer_id DESC, contract_id DESC
    LIMIT {LIMIT}
    FORMAT JSONEachRow
    """
    return ch_query_json(sql)


def main() -> int:
    write_tsv_header(TSV)
    result = EndpointResult(endpoint=ENDPOINT)
    started = time.monotonic()

    cursor = None
    page_idx = 0
    all_issuers: set[int] = set()
    all_contracts: set[int] = set()
    seen_tuples: list[tuple] = []

    while page_idx < PAGES:
        rows = fetch_page(cursor)
        if not rows:
            break

        for r in rows:
            t = (
                int(r["asset_type"]),
                r["asset_code"] or "",
                int(r["issuer_id"]),
                int(r["contract_id"]),
            )
            seen_tuples.append(t)
            if t[2] != 0:
                all_issuers.add(t[2])
            if t[3] != 0:
                all_contracts.add(t[3])

        last = rows[-1]
        cursor = (
            int(last["asset_type"]),
            last["asset_code"] or "",
            int(last["issuer_id"]),
            int(last["contract_id"]),
        )
        page_idx += 1

    diffs: list[str] = []
    result.sample_size = len(seen_tuples)

    # Monotonic cursor invariant.
    if assert_monotonic_desc_loose(seen_tuples):
        result.record_field("cursor_monotonic", "pass")
    else:
        result.record_field("cursor_monotonic", "fail")
        diffs.append("cursor not monotonic DESC across pages")

    # Issuer FK resolution.
    if all_issuers:
        ids = ",".join(str(i) for i in all_issuers)
        sql = (
            f"SELECT count() FROM accounts FINAL WHERE id IN ({ids}) FORMAT TabSeparated"
        )
        from phase_d_common import ch_scalar
        present = int(ch_scalar(sql) or 0)
        if present == len(all_issuers):
            result.record_field("issuer_fk", "pass")
        else:
            result.record_field("issuer_fk", "fail")
            diffs.append(f"issuer_fk: {present}/{len(all_issuers)} resolved")
    else:
        result.record_field("issuer_fk", "pass")  # vacuously true

    # Contract FK resolution.
    if all_contracts:
        ids = ",".join(str(i) for i in all_contracts)
        sql = (
            f"SELECT count() FROM soroban_contracts FINAL WHERE id IN ({ids}) "
            f"FORMAT TabSeparated"
        )
        from phase_d_common import ch_scalar
        present = int(ch_scalar(sql) or 0)
        if present == len(all_contracts):
            result.record_field("contract_fk", "pass")
        else:
            result.record_field("contract_fk", "fail")
            diffs.append(f"contract_fk: {present}/{len(all_contracts)} resolved")
    else:
        result.record_field("contract_fk", "pass")

    if diffs:
        dump_diff(ENDPOINT, "walk", {"sample_size": len(seen_tuples)}, None, diffs)

    pass_n = sum(1 for v in result.fields.values() if v.pass_count > 0)
    fail_n = sum(1 for v in result.fields.values() if v.fail_count > 0)
    append_tsv_row(TSV, ENDPOINT, "walk", pass_n, 0, fail_n, ";".join(diffs)[:500])

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_d_e08_summary.json"
    summary.write_text(json.dumps({
        "endpoint": ENDPOINT,
        "sample_size": result.sample_size,
        "pages_walked": page_idx,
        "issuers_checked": len(all_issuers),
        "contracts_checked": len(all_contracts),
        "pass_total": result.pass_total,
        "fail_total": result.fail_total,
        "tolerance_total": result.tolerance_total,
        "elapsed_ms": result.elapsed_ms,
        "fields": {
            k: {"pass": v.pass_count, "tolerance": v.tolerance_count, "fail": v.fail_count}
            for k, v in result.fields.items()
        },
    }, indent=2))

    print(f"[E08] done: pages={page_idx} rows={len(seen_tuples)} "
          f"issuers={len(all_issuers)} contracts={len(all_contracts)} "
          f"pass={result.pass_total} fail={result.fail_total} "
          f"elapsed={result.elapsed_ms}ms", file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
