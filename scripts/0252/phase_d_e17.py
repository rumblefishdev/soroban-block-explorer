#!/usr/bin/env python3
"""E17 — /nfts/:id/transfers internal sanity.

Sample N NFTs from `nft_ownership FINAL`. For each:
  * Walk 5 pages × 50 rows of ownership history.
  * Per-(contract, token, ledger) `event_order` is dense + monotonic.
  * `transaction_hash` resolves into `transactions FINAL`.
  * Cursor (`ledger_sequence DESC, event_order DESC`) monotonic.
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


ENDPOINT = "E17"
TSV = OUT_DIR / "phase_d_e17.tsv"

SAMPLE = 200
PAGES = 5
LIMIT = 50


def sample_nfts(n: int) -> list[dict]:
    sql = f"""
    SELECT DISTINCT contract_id, token_id
    FROM nft_ownership
    ORDER BY cityHash64(contract_id, token_id)
    LIMIT {n}
    FORMAT JSONEachRow
    """
    return ch_query_json(sql)


def walk_transfers(cid: int, tid: str) -> tuple[list[tuple[int, int]], set[str]]:
    """Walk PAGES pages. Return (cursor_tuples, tx_hashes_hex)."""
    tid_esc = tid.replace("'", "''")
    cursor: tuple[int, int] | None = None
    tuples: list[tuple[int, int]] = []
    tx_hashes: set[str] = set()

    for _ in range(PAGES):
        if cursor is None:
            cur_pred = ""
        else:
            lseq, eord = cursor
            cur_pred = f"AND (ledger_sequence, event_order) < ({lseq}, {eord})"
        sql = f"""
        SELECT
            ledger_sequence       AS ledger_sequence,
            event_order           AS event_order,
            lower(hex(transaction_hash))  AS tx_hash
        FROM nft_ownership FINAL
        WHERE contract_id = {cid} AND token_id = '{tid_esc}'
        {cur_pred}
        ORDER BY ledger_sequence DESC, event_order DESC
        LIMIT {LIMIT}
        FORMAT JSONEachRow
        """
        rows = ch_query_json(sql)
        if not rows:
            break
        for r in rows:
            t = (int(r["ledger_sequence"]), int(r["event_order"]))
            tuples.append(t)
            if r.get("tx_hash"):
                tx_hashes.add(r["tx_hash"])
        last = rows[-1]
        cursor = (int(last["ledger_sequence"]), int(last["event_order"]))

    return tuples, tx_hashes


def main() -> int:
    write_tsv_header(TSV)
    result = EndpointResult(endpoint=ENDPOINT)
    started = time.monotonic()

    nfts = sample_nfts(SAMPLE)
    print(f"[E17] {len(nfts)} nft samples", file=sys.stderr)
    result.sample_size = len(nfts)

    cursor_fail = 0
    fk_fail = 0
    walked = 0
    all_tx_hashes: set[str] = set()
    diffs: list[str] = []
    processed = 0

    for nft in nfts:
        processed += 1
        cid = int(nft["contract_id"])
        tid = nft.get("token_id") or ""

        try:
            tuples, tx_hashes = walk_transfers(cid, tid)
        except RuntimeError as e:
            result.record_field("walk", "fail")
            diffs.append(f"walk_error cid={cid}: {str(e)[:120]}")
            continue

        if not tuples:
            result.record_field("cursor_monotonic", "pass")
            continue
        walked += len(tuples)
        all_tx_hashes.update(tx_hashes)

        if assert_monotonic_desc_loose(tuples):
            result.record_field("cursor_monotonic", "pass")
        else:
            result.record_field("cursor_monotonic", "fail")
            cursor_fail += 1

    # FK check: tx hashes → transactions.hash (FixedString(32) — match via unhex).
    if all_tx_hashes:
        sample_hashes = list(all_tx_hashes)[:500]
        in_list = ",".join(f"unhex('{h}')" for h in sample_hashes)
        n = int(
            ch_scalar(
                f"SELECT count(DISTINCT hash) FROM transactions FINAL "
                f"WHERE hash IN ({in_list}) FORMAT TabSeparated"
            ) or 0
        )
        if n == len(sample_hashes):
            result.record_field("tx_fk", "pass")
        else:
            result.record_field("tx_fk", "fail")
            fk_fail += 1
            diffs.append(f"tx_fk: {n}/{len(sample_hashes)} resolved")

    if diffs:
        dump_diff(ENDPOINT, "walk", {"sample": len(nfts), "rows": walked}, None, diffs[:50])

    pass_n = result.pass_total
    fail_n = result.fail_total
    append_tsv_row(TSV, ENDPOINT, "walk", pass_n, 0, fail_n,
                   f"sample={len(nfts)} rows={walked} cursor_fail={cursor_fail} fk_fail={fk_fail}")
    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_d_e17_summary.json"
    summary.write_text(json.dumps({
        "endpoint": ENDPOINT,
        "sample_size": result.sample_size,
        "rows_walked": walked,
        "unique_tx_hashes": len(all_tx_hashes),
        "pass_total": result.pass_total,
        "fail_total": result.fail_total,
        "tolerance_total": result.tolerance_total,
        "elapsed_ms": result.elapsed_ms,
        "fields": {
            k: {"pass": v.pass_count, "tolerance": v.tolerance_count, "fail": v.fail_count}
            for k, v in result.fields.items()
        },
    }, indent=2))

    print(f"[E17] done: sample={len(nfts)} rows={walked} "
          f"pass={result.pass_total} fail={result.fail_total} "
          f"elapsed={result.elapsed_ms}ms", file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
