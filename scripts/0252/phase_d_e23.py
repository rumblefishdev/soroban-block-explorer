#!/usr/bin/env python3
"""E23 — /liquidity-pools/:id/participants internal sanity.

Sample N pools. For each:
  * Run participant query, walk 3 pages × 50.
  * `shares DESC` monotonic across pages.
  * `account_id` FK resolves into `accounts FINAL`.
  * `sum(shares)` over the page <= total_shares from latest snapshot
    (cannot exceed; equality is the expected steady state).
"""

from __future__ import annotations

import json
import random
import sys
import time
from decimal import Decimal, InvalidOperation
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from common import (
    OUT_DIR,
    EndpointResult,
    append_tsv_row,
    ch_query_json,
    dump_diff,
    load_samples,
    write_tsv_header,
)
from phase_d_common import ch_scalar


ENDPOINT = "E23"
TSV = OUT_DIR / "phase_d_e23.tsv"

SAMPLE = 300
PAGES = 3
LIMIT = 50


def _dec_plain(d: Decimal) -> str:
    """Format a Decimal as a plain (non-scientific) string so CH's
    `toDecimal128(literal, 7)` can parse it. `str(Decimal('0E-7'))` is
    `'0E-7'`, which CH refuses; `format(d, 'f')` gives `'0.0000000'`.
    """
    try:
        return format(d, 'f')
    except (InvalidOperation, ValueError):
        return "0"


def fetch_participants(pool_hex: str, cursor: tuple | None) -> list[dict]:
    if cursor is None:
        cur_pred = ""
    else:
        shares, aid = cursor
        # Both literals cast explicitly so CH does not have to guess a
        # supertype for the row comparison
        # `(Decimal(38,7), Int64) < (UInt8, Int64)` (Code 386 in
        # CH 26.3.10).
        cur_pred = (
            f"AND (shares, account_id) < "
            f"(toDecimal128('{_dec_plain(shares)}', 7), toInt64({aid}))"
        )
    sql = f"""
    SELECT
        toString(shares)                  AS shares,
        account_id                        AS account_id
    FROM lp_positions FINAL
    WHERE pool_id = unhex('{pool_hex}')
      {cur_pred}
    ORDER BY shares DESC, account_id DESC
    LIMIT {LIMIT}
    FORMAT JSONEachRow
    """
    return ch_query_json(sql)


def latest_total_shares(pool_hex: str) -> Decimal | None:
    sql = f"""
    SELECT toString(argMax(total_shares, ledger_sequence)) AS total
    FROM liquidity_pool_snapshots FINAL
    WHERE pool_id = unhex('{pool_hex}')
    FORMAT TabSeparated
    """
    out = ch_scalar(sql)
    if not out:
        return None
    try:
        return Decimal(out)
    except (InvalidOperation, ValueError):
        return None


def main() -> int:
    write_tsv_header(TSV)
    result = EndpointResult(endpoint=ENDPOINT)
    started = time.monotonic()
    random.seed(42)

    pools = load_samples("samples_pools.txt")
    random.shuffle(pools)
    pools = pools[:SAMPLE]
    print(f"[E23] {len(pools)} pool samples", file=sys.stderr)
    result.sample_size = len(pools)

    diffs: list[str] = []
    processed = 0
    walked = 0

    for pool_hex in pools:
        processed += 1
        cursor = None
        page_shares: list[Decimal] = []
        accounts: set[int] = set()

        for _ in range(PAGES):
            try:
                rows = fetch_participants(pool_hex, cursor)
            except RuntimeError as e:
                result.record_field("query_runs", "fail")
                diffs.append(f"query_error pool={pool_hex[:10]}: {str(e)[:80]}")
                rows = []
                break
            if not rows:
                break
            for r in rows:
                try:
                    s = Decimal(r["shares"])
                except (InvalidOperation, ValueError):
                    s = Decimal("-1")
                page_shares.append(s)
                accounts.add(int(r["account_id"]))
            last = rows[-1]
            try:
                cursor = (Decimal(last["shares"]), int(last["account_id"]))
            except (InvalidOperation, ValueError):
                cursor = None
                break

        if not page_shares:
            continue
        walked += len(page_shares)
        result.record_field("query_runs", "pass")

        # shares DESC monotonic.
        is_desc = all(a >= b for a, b in zip(page_shares, page_shares[1:]))
        result.record_field("shares_monotonic", "pass" if is_desc else "fail")
        if not is_desc:
            diffs.append(f"shares not monotonic pool={pool_hex[:10]}")

        # FK accounts.
        if accounts:
            ids = ",".join(str(a) for a in accounts)
            n = int(
                ch_scalar(
                    f"SELECT count() FROM accounts FINAL WHERE id IN ({ids}) "
                    f"FORMAT TabSeparated"
                ) or 0
            )
            if n == len(accounts):
                result.record_field("account_fk", "pass")
            else:
                result.record_field("account_fk", "fail")
                diffs.append(f"account_fk pool={pool_hex[:10]}: {n}/{len(accounts)}")

        # sum(page_shares) <= total_shares.
        total = latest_total_shares(pool_hex)
        if total is not None:
            page_sum = sum(s for s in page_shares if s >= 0)
            if page_sum <= total + Decimal("0.0000001"):
                result.record_field("shares_bounded_by_total", "pass")
            else:
                result.record_field("shares_bounded_by_total", "fail")
                diffs.append(f"shares_bounded pool={pool_hex[:10]} page_sum={page_sum} total={total}")

        if processed % 50 == 0:
            print(f"[E23] {processed}/{len(pools)} pools, rows_seen={walked}",
                  file=sys.stderr)

    if diffs:
        dump_diff(ENDPOINT, "walk", {"sample": len(pools), "rows": walked}, None, diffs[:50])

    pass_n = result.pass_total
    fail_n = result.fail_total
    append_tsv_row(TSV, ENDPOINT, "walk", pass_n, 0, fail_n, f"sample={len(pools)} rows={walked}")
    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_d_e23_summary.json"
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

    print(f"[E23] done: sample={len(pools)} rows={walked} "
          f"pass={result.pass_total} fail={result.fail_total} "
          f"elapsed={result.elapsed_ms}ms", file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
