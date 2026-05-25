#!/usr/bin/env python3
"""E16 — /nfts/:id detail seek internal sanity.

Sample N NFTs from `nfts FINAL` (random hash bucket). For each:
  * direct PK seek returns exactly 1 row
  * contract_id resolves in `soroban_contracts FINAL`
  * current_owner_id (if non-zero) resolves in `accounts FINAL`
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


ENDPOINT = "E16"
TSV = OUT_DIR / "phase_d_e16.tsv"

SAMPLE = 500


def sample_nfts(n: int) -> list[dict]:
    sql = f"""
    SELECT contract_id, token_id, current_owner_id
    FROM nfts FINAL
    ORDER BY cityHash64(contract_id, token_id)
    LIMIT {n}
    FORMAT JSONEachRow
    """
    return ch_query_json(sql)


def seek_nft(cid: int, tid: str) -> dict | None:
    tid_esc = tid.replace("'", "''")
    sql = f"""
    SELECT contract_id, token_id, current_owner_id
    FROM nfts FINAL
    WHERE contract_id = {cid} AND token_id = '{tid_esc}'
    LIMIT 1
    FORMAT JSONEachRow
    """
    rows = ch_query_json(sql)
    return rows[0] if rows else None


def main() -> int:
    write_tsv_header(TSV)
    result = EndpointResult(endpoint=ENDPOINT)
    started = time.monotonic()

    nfts = sample_nfts(SAMPLE)
    print(f"[E16] {len(nfts)} nft samples", file=sys.stderr)
    result.sample_size = len(nfts)

    contracts: set[int] = set()
    owners: set[int] = set()
    diffs: list[str] = []
    seek_fail = 0

    for nft in nfts:
        cid = int(nft["contract_id"])
        tid = nft.get("token_id") or ""
        owner = int(nft.get("current_owner_id") or 0)
        if cid:
            contracts.add(cid)
        if owner:
            owners.add(owner)

        seek = seek_nft(cid, tid)
        if seek is None:
            result.record_field("seek_returns_one", "fail")
            seek_fail += 1
            diffs.append(f"seek_miss cid={cid} tid={tid!r}")
        else:
            result.record_field("seek_returns_one", "pass")

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
            diffs.append(f"contract_fk: {n}/{len(contracts)}")
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
            diffs.append(f"owner_fk: {n}/{len(owners)}")
    else:
        result.record_field("owner_fk", "pass")

    if diffs:
        dump_diff(ENDPOINT, "seek", {"sample": len(nfts), "seek_fail": seek_fail}, None, diffs[:50])

    pass_n = result.pass_total
    fail_n = result.fail_total
    append_tsv_row(TSV, ENDPOINT, "seek", pass_n, 0, fail_n,
                   f"sample={len(nfts)} seek_fail={seek_fail}")
    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_d_e16_summary.json"
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

    print(f"[E16] done: sample={len(nfts)} seek_fail={seek_fail} "
          f"pass={result.pass_total} fail={result.fail_total} "
          f"elapsed={result.elapsed_ms}ms", file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
