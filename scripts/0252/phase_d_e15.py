#!/usr/bin/env python3
"""E15 — /nfts list internal sanity.

Paginated walk by `(contract_id DESC, token_id DESC)`. Checks:
  * tuple cursor strictly monotonic DESC
  * every `contract_id` resolves in `soroban_contracts FINAL`
  * every non-NULL `current_owner` resolves in `accounts FINAL`
  * no orphaned rows (contract_id=0 / empty token_id)

30 pages × 50 rows = 1500 NFT rows.
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
from phase_d_common import assert_monotonic_desc_loose, ch_scalar


ENDPOINT = "E15"
TSV = OUT_DIR / "phase_d_e15.tsv"

PAGES = 30
LIMIT = 50


def fetch_page(cursor: tuple | None) -> list[dict]:
    if cursor is None:
        where = "1=1"
    else:
        cid, tid = cursor
        tid_esc = tid.replace("'", "''")
        where = f"(contract_id, token_id) < ({cid}, '{tid_esc}')"
    sql = f"""
    SELECT
        contract_id                  AS contract_id,
        token_id                     AS token_id,
        current_owner_id             AS current_owner_id
    FROM nfts FINAL
    WHERE {where}
    ORDER BY contract_id DESC, token_id DESC
    LIMIT {LIMIT}
    FORMAT JSONEachRow
    """
    return ch_query_json(sql)


def main() -> int:
    write_tsv_header(TSV)
    result = EndpointResult(endpoint=ENDPOINT)
    started = time.monotonic()

    cursor = None
    seen: list[tuple] = []
    contracts: set[int] = set()
    owners: set[int] = set()
    diffs: list[str] = []

    for _ in range(PAGES):
        rows = fetch_page(cursor)
        if not rows:
            break
        for r in rows:
            cid = int(r["contract_id"])
            tid = r["token_id"] or ""
            seen.append((cid, tid))
            if cid:
                contracts.add(cid)
            oid = int(r.get("current_owner_id") or 0)
            if oid:
                owners.add(oid)
            if not tid or cid == 0:
                diffs.append(f"orphan row cid={cid} tid={tid!r}")
        last = rows[-1]
        cursor = (int(last["contract_id"]), last["token_id"] or "")

    result.sample_size = len(seen)

    if assert_monotonic_desc_loose(seen):
        result.record_field("cursor_monotonic", "pass")
    else:
        result.record_field("cursor_monotonic", "fail")
        diffs.append("cursor not monotonic DESC")

    if contracts:
        ids = ",".join(str(i) for i in contracts)
        n = int(
            ch_scalar(
                f"SELECT count() FROM soroban_contracts FINAL WHERE id IN ({ids}) "
                f"FORMAT TabSeparated"
            ) or 0
        )
        if n == len(contracts):
            result.record_field("contract_fk", "pass")
        else:
            result.record_field("contract_fk", "fail")
            diffs.append(f"contract_fk: {n}/{len(contracts)} resolved")
    else:
        result.record_field("contract_fk", "pass")

    if owners:
        ids = ",".join(str(i) for i in owners)
        n = int(
            ch_scalar(
                f"SELECT count() FROM accounts FINAL WHERE id IN ({ids}) "
                f"FORMAT TabSeparated"
            ) or 0
        )
        if n == len(owners):
            result.record_field("owner_fk", "pass")
        else:
            result.record_field("owner_fk", "fail")
            diffs.append(f"owner_fk: {n}/{len(owners)} resolved")
    else:
        result.record_field("owner_fk", "pass")

    if not diffs:
        result.record_field("no_orphan_rows", "pass")
    else:
        result.record_field("no_orphan_rows", "fail")

    if diffs:
        dump_diff(ENDPOINT, "walk", {"sample": len(seen)}, None, diffs[:50])

    pass_n = sum(v.pass_count for v in result.fields.values())
    fail_n = sum(v.fail_count for v in result.fields.values())
    append_tsv_row(TSV, ENDPOINT, "walk", pass_n, 0, fail_n, ";".join(diffs)[:500])
    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_d_e15_summary.json"
    summary.write_text(json.dumps({
        "endpoint": ENDPOINT,
        "sample_size": result.sample_size,
        "contracts_checked": len(contracts),
        "owners_checked": len(owners),
        "pass_total": result.pass_total,
        "fail_total": result.fail_total,
        "tolerance_total": result.tolerance_total,
        "elapsed_ms": result.elapsed_ms,
        "fields": {
            k: {"pass": v.pass_count, "tolerance": v.tolerance_count, "fail": v.fail_count}
            for k, v in result.fields.items()
        },
    }, indent=2))

    print(f"[E15] done: rows={len(seen)} contracts={len(contracts)} "
          f"owners={len(owners)} pass={result.pass_total} fail={result.fail_total} "
          f"elapsed={result.elapsed_ms}ms", file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
