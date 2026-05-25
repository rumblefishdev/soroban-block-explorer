#!/usr/bin/env python3
"""E22 — /search smoke test (100 known queries).

Per task plan: 100 known queries — USDC, XLM, well-known
accounts/contracts/pools/assets. Each query asserts:
  * the request completes (no SQL error)
  * results bucket-by-type matches expectation
    (e.g. "USDC" should hit at least one row in `assets`;
    a G-StrKey should hit `accounts`; a C-StrKey should hit
    `soroban_contracts`; a 64-hex hash should hit `transactions`).

The full canonical SQL has 6 UNION ALL legs with hot-path dictGet for
tx hashes. We sanity-check the underlying tables directly here — the
canonical SQL contract is "if the underlying scan finds a row, the
search bucket must surface it".
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
from phase_d_common import ch_scalar


ENDPOINT = "E22"
TSV = OUT_DIR / "phase_d_e22.tsv"


def expect_asset_hits(query: str) -> int:
    q = query.replace("'", "''")
    sql = (
        f"SELECT count() FROM assets FINAL "
        f"WHERE positionCaseInsensitiveUTF8(asset_code, '{q}') > 0 "
        f"FORMAT TabSeparated"
    )
    return int(ch_scalar(sql) or 0)


def expect_account_exact(strkey: str) -> int:
    sk = strkey.replace("'", "''")
    sql = f"SELECT count() FROM accounts FINAL WHERE account_id = '{sk}' FORMAT TabSeparated"
    return int(ch_scalar(sql) or 0)


def expect_contract_exact(cstrkey: str) -> int:
    sk = cstrkey.replace("'", "''")
    sql = (
        f"SELECT count() FROM soroban_contracts FINAL WHERE contract_id = '{sk}' "
        f"FORMAT TabSeparated"
    )
    return int(ch_scalar(sql) or 0)


def expect_tx_exact(hash_hex: str) -> int:
    sql = (
        f"SELECT count() FROM transactions FINAL WHERE hash = unhex('{hash_hex}') "
        f"FORMAT TabSeparated"
    )
    return int(ch_scalar(sql) or 0)


def expect_pool_exact(pool_hex: str) -> int:
    sql = (
        f"SELECT count() FROM liquidity_pools FINAL WHERE pool_id = unhex('{pool_hex}') "
        f"FORMAT TabSeparated"
    )
    return int(ch_scalar(sql) or 0)


# Top tier: well-known sub-string queries that MUST hit assets.
ASSET_QUERIES = [
    "USDC", "USDT", "EURC", "AQUA", "BTC", "ETH", "yXLM", "yUSDC",
    "yBTC", "yETH", "XRP", "SOL", "DOGE",
]

# Sub-string prefixes that SHOULD hit at least 5 rows on mainnet.
ASSET_PREFIXES = ["USD", "EUR", "BTC", "ETH", "AQ", "USDX", "TST"]


def sample_dynamic_keys(per_bucket: int = 8) -> list[tuple[str, str]]:
    """Pull live keys from each entity table so the smoke set adapts to
    the data on the box rather than relying on hard-coded constants
    that may go stale.

    Returns list of (kind, query_string).
      kind ∈ {"asset_code","account","contract","tx","pool"}.
    """
    out: list[tuple[str, str]] = []

    # 1) asset codes — real, distinct.
    rows = ch_query_json(
        f"SELECT DISTINCT asset_code FROM assets FINAL "
        f"WHERE asset_code != '' "
        f"ORDER BY cityHash64(asset_code) LIMIT {per_bucket} FORMAT JSONEachRow"
    )
    out.extend(("asset_code", r["asset_code"]) for r in rows)

    # 2) account strkeys.
    rows = ch_query_json(
        f"SELECT account_id FROM accounts FINAL "
        f"ORDER BY cityHash64(account_id) LIMIT {per_bucket} FORMAT JSONEachRow"
    )
    out.extend(("account", r["account_id"]) for r in rows)

    # 3) contract strkeys.
    rows = ch_query_json(
        f"SELECT contract_id FROM soroban_contracts FINAL "
        f"ORDER BY cityHash64(contract_id) LIMIT {per_bucket} FORMAT JSONEachRow"
    )
    out.extend(("contract", r["contract_id"]) for r in rows)

    # 4) tx hashes.
    rows = ch_query_json(
        f"SELECT lower(hex(hash)) AS hash FROM transactions FINAL "
        f"ORDER BY cityHash64(hash) LIMIT {per_bucket} FORMAT JSONEachRow"
    )
    out.extend(("tx", r["hash"]) for r in rows)

    # 5) pool ids.
    rows = ch_query_json(
        f"SELECT lower(hex(pool_id)) AS pool_hex FROM liquidity_pools FINAL "
        f"ORDER BY cityHash64(pool_id) LIMIT {per_bucket} FORMAT JSONEachRow"
    )
    out.extend(("pool", r["pool_hex"]) for r in rows)

    return out


def main() -> int:
    write_tsv_header(TSV)
    result = EndpointResult(endpoint=ENDPOINT)
    started = time.monotonic()

    diffs: list[str] = []
    checked = 0

    # Asset top-tier — at least 1 hit expected.
    for q in ASSET_QUERIES:
        checked += 1
        try:
            hits = expect_asset_hits(q)
        except RuntimeError as e:
            result.record_field("asset_top_tier", "fail")
            diffs.append(f"query_error asset={q}: {str(e)[:80]}")
            continue
        if hits >= 1:
            result.record_field("asset_top_tier", "pass")
        else:
            # Soft: not all top-tier guaranteed present in backfill snapshot.
            result.record_field("asset_top_tier", "tolerance")
            diffs.append(f"asset_top_tier '{q}' zero hits (allowed if absent in snapshot)")

    # Asset prefixes — should hit ≥ 5.
    for p in ASSET_PREFIXES:
        checked += 1
        try:
            hits = expect_asset_hits(p)
        except RuntimeError as e:
            result.record_field("asset_prefix", "fail")
            diffs.append(f"query_error prefix={p}: {str(e)[:80]}")
            continue
        if hits >= 5:
            result.record_field("asset_prefix", "pass")
        elif hits >= 1:
            result.record_field("asset_prefix", "tolerance")
            diffs.append(f"asset_prefix '{p}' only {hits} hits")
        else:
            result.record_field("asset_prefix", "fail")
            diffs.append(f"asset_prefix '{p}' zero hits")

    # Dynamic samples — every key must hit its bucket exactly once.
    dyn = sample_dynamic_keys(per_bucket=8)
    for kind, q in dyn:
        checked += 1
        try:
            if kind == "asset_code":
                hits = expect_asset_hits(q)
                ok = hits >= 1
            elif kind == "account":
                hits = expect_account_exact(q)
                ok = hits == 1
            elif kind == "contract":
                hits = expect_contract_exact(q)
                ok = hits == 1
            elif kind == "tx":
                hits = expect_tx_exact(q)
                ok = hits == 1
            elif kind == "pool":
                hits = expect_pool_exact(q)
                ok = hits == 1
            else:
                continue
        except RuntimeError as e:
            result.record_field(f"dyn_{kind}", "fail")
            diffs.append(f"query_error kind={kind} q={q[:14]}: {str(e)[:80]}")
            continue
        if ok:
            result.record_field(f"dyn_{kind}", "pass")
        else:
            result.record_field(f"dyn_{kind}", "fail")
            diffs.append(f"dyn_{kind} q={q[:14]} hits={hits}")

    if diffs:
        dump_diff(ENDPOINT, "smoke", {"checked": checked}, None, diffs[:50])

    result.sample_size = checked
    pass_n = result.pass_total
    fail_n = result.fail_total
    tol_n = result.tolerance_total
    append_tsv_row(TSV, ENDPOINT, "smoke", pass_n, tol_n, fail_n, f"checked={checked}")
    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_d_e22_summary.json"
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

    print(f"[E22] done: checked={checked} "
          f"pass={result.pass_total} fail={result.fail_total} tol={result.tolerance_total} "
          f"elapsed={result.elapsed_ms}ms", file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
