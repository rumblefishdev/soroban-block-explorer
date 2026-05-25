#!/usr/bin/env python3
"""Phase F — per-endpoint latency profile.

For each of the 23 ClickHouse read endpoints, run a representative
query 510 times sequentially, drop the first 10 (cold-cache
warm-up — captured separately as `cold_first_ms`) and report p50 /
p95 / p99 / max / min over runs 11-510.

Per task 0252 plan §"Phase F":
  * Cold first measured separately (single value, run #1).
  * Warm aggregates over runs 11..510.
  * Verdict thresholds:
      p95 < 100 ms      FAST
      100 ≤ p95 < 500   OK
      500 ≤ p95 < 1500  SLOW
      p95 ≥ 1500        FAIL

Output: TSV at `/tmp/sbe-artifacts/0252/phase_f_perf.tsv` —
endpoint, file, cold_first_ms, p50_warm, p95_warm, p99_warm,
max_ms, min_ms, n_warm, verdict.

Realistic param values are hand-picked per endpoint from the
sample pools (`samples_ledgers.txt`, `samples_pools.txt`,
`samples_contracts.txt`) + a few hot CH constants. Each query
mirrors the shape exercised by the canonical SQL in
`docs/architecture/database-schema/endpoint-queries-clickhouse/`.
"""

from __future__ import annotations

import json
import os
import random
import statistics
import sys
import time
from pathlib import Path
from typing import Callable

sys.path.insert(0, str(Path(__file__).parent))
from common import (
    OUT_DIR,
    SAMPLE_DIR,
    ch_query,
)


TSV = OUT_DIR / "phase_f_perf.tsv"
RUNS = int(os.environ.get("SBE_F_RUNS", "510"))
WARM_DROP = int(os.environ.get("SBE_F_WARM_DROP", "10"))


# ---------- realistic param picks ---------------------------------------

def _first_sample(filename: str) -> str:
    p = SAMPLE_DIR / filename
    if not p.exists():
        return ""
    for line in p.read_text().splitlines():
        if line.strip():
            return line.strip()
    return ""


def _pick_ledger() -> int:
    p = SAMPLE_DIR / "samples_ledgers.txt"
    if p.exists():
        ledgers = [int(l) for l in p.read_text().splitlines() if l.strip()]
        random.seed(7)
        for l in random.sample(ledgers, min(50, len(ledgers))):
            if 56_657_428 <= l <= 62_525_000:
                return l
    return 60_000_000


def _pick_tx_hash() -> str:
    """Random tx hash from a retention-valid ledger."""
    sql = (
        "SELECT lower(hex(hash)) FROM transaction_hash_index "
        "WHERE ledger_sequence BETWEEN 58000000 AND 62000000 "
        "ORDER BY cityHash64(hash) LIMIT 1 FORMAT TabSeparated"
    )
    try:
        return ch_query(sql).strip() or "0" * 64
    except RuntimeError:
        return "0" * 64


def _pick_contract() -> str:
    return _first_sample("samples_contracts.txt") or "C" + "A" * 55


def _pick_pool_hex() -> str:
    return _first_sample("samples_pools.txt") or "0" * 64


def _pick_account() -> str:
    return _first_sample("samples_accounts.txt") or (
        "G" + "A" * 55
    )


# Materialised at module-import time so each endpoint's SQL string
# is built once.
LEDGER = _pick_ledger()
TX_HASH = _pick_tx_hash()
CONTRACT = _pick_contract()
POOL = _pick_pool_hex()
ACCOUNT = _pick_account()
PART = LEDGER // 500_000


# ---------- per-endpoint queries ----------------------------------------
#
# Each value is a no-arg callable returning the SQL string. Callable so
# we can rebuild a fresh string per run if needed (none currently do).
# `FORMAT Null` is appended so CH skips serialisation — we measure
# query execution time, not row marshalling.

ENDPOINTS: dict[str, tuple[str, Callable[[], str]]] = {
    "E01": (
        "01_get_network_stats.sql",
        lambda: """
        SELECT
            (SELECT max(sequence) FROM ledgers),
            (SELECT count() FROM ledgers
              WHERE closed_at >= now() - INTERVAL 60 SECOND),
            (SELECT total_rows FROM system.tables
              WHERE database = currentDatabase() AND name = 'accounts'),
            (SELECT total_rows FROM system.tables
              WHERE database = currentDatabase() AND name = 'soroban_contracts'),
            now()
        FORMAT Null
        """,
    ),
    "E02": (
        "02_get_transactions_list.sql",
        lambda: f"""
        SELECT t.id
        FROM (
            SELECT * FROM transactions FINAL
            WHERE intDiv(ledger_sequence, 500000) = {PART}
              AND (ledger_sequence, id) < ({LEDGER}, 0x7FFFFFFFFFFFFFFF)
            ORDER BY ledger_sequence DESC, id DESC
            LIMIT 50
        ) t
        JOIN accounts src ON src.id = t.source_id
        FORMAT Null
        """,
    ),
    "E03": (
        "03_get_transactions_by_hash.sql",
        lambda: f"""
        WITH (
          SELECT ledger_sequence FROM transaction_hash_index
           WHERE hash = unhex('{TX_HASH}') LIMIT 1
        ) AS lseq
        SELECT t.id, t.hash, t.ledger_sequence, t.fee_charged
        FROM transactions AS t FINAL
        LEFT JOIN accounts AS a FINAL ON a.id = t.source_id
        WHERE t.ledger_sequence = lseq AND t.hash = unhex('{TX_HASH}')
        FORMAT Null
        """,
    ),
    "E04": (
        "04_get_ledgers_list.sql",
        lambda: f"""
        SELECT sequence, closed_at, transaction_count
        FROM ledgers
        WHERE (closed_at, sequence) < (now(), {LEDGER + 1})
        ORDER BY closed_at DESC, sequence DESC
        LIMIT 50
        FORMAT Null
        """,
    ),
    "E05": (
        "05_get_ledgers_by_sequence.sql",
        lambda: f"""
        SELECT sequence, lower(hex(hash)), closed_at, transaction_count
        FROM ledgers WHERE sequence = {LEDGER}
        FORMAT Null
        """,
    ),
    "E06": (
        "06_get_accounts_by_id.sql",
        lambda: f"""
        SELECT a.id, a.account_id, a.last_seen_ledger
        FROM accounts AS a FINAL
        WHERE a.account_id = '{ACCOUNT}'
        FORMAT Null
        """,
    ),
    "E07": (
        "07_get_accounts_transactions.sql",
        lambda: f"""
        WITH (SELECT id FROM accounts FINAL
               WHERE account_id = '{ACCOUNT}' LIMIT 1) AS aid
        SELECT t.id, t.ledger_sequence, lower(hex(t.hash))
        FROM transaction_participants AS tp
        INNER JOIN transactions AS t FINAL
            ON t.id = tp.transaction_id AND t.ledger_sequence = tp.ledger_sequence
        WHERE tp.account_id = aid
          AND intDiv(t.ledger_sequence, 500000) = {PART}
        ORDER BY t.ledger_sequence DESC, t.id DESC
        LIMIT 50
        FORMAT Null
        """,
    ),
    "E08": (
        "08_get_assets_list.sql",
        lambda: """
        SELECT asset_type, asset_code, issuer_id, contract_id
        FROM assets FINAL
        ORDER BY asset_type DESC, asset_code DESC, issuer_id DESC, contract_id DESC
        LIMIT 50
        FORMAT Null
        """,
    ),
    "E09": (
        "09_get_assets_by_id.sql",
        lambda: f"""
        SELECT asset_type, asset_code, issuer_id, contract_id
        FROM assets FINAL
        WHERE contract_id = (SELECT id FROM soroban_contracts FINAL
                              WHERE contract_id = '{CONTRACT}' LIMIT 1)
        LIMIT 1
        FORMAT Null
        """,
    ),
    "E10": (
        "10_get_assets_transactions.sql",
        lambda: f"""
        SELECT DISTINCT oa.ledger_sequence, oa.transaction_id
        FROM operations_appearances AS oa
        WHERE oa.contract_id = (SELECT id FROM soroban_contracts FINAL
                                 WHERE contract_id = '{CONTRACT}' LIMIT 1)
          AND intDiv(oa.ledger_sequence, 500000) = {PART}
        ORDER BY oa.ledger_sequence DESC, oa.transaction_id DESC
        LIMIT 50
        FORMAT Null
        """,
    ),
    "E11": (
        "11_get_contracts_by_id.sql",
        lambda: f"""
        SELECT sc.contract_id, sc.is_sac, sc.deployer_id, sc.deployed_at_ledger
        FROM soroban_contracts AS sc FINAL
        WHERE sc.contract_id = '{CONTRACT}'
        LIMIT 1
        FORMAT Null
        """,
    ),
    "E12": (
        "12_get_contracts_interface.sql",
        lambda: f"""
        SELECT sc.contract_id, lower(hex(sc.wasm_hash)), ifNull(wim.metadata, '{{}}')
        FROM soroban_contracts AS sc FINAL
        LEFT JOIN wasm_interface_metadata AS wim ON wim.wasm_hash = sc.wasm_hash
        WHERE sc.contract_id = '{CONTRACT}'
        LIMIT 1
        FORMAT Null
        """,
    ),
    "E13": (
        "13_get_contracts_invocations.sql",
        lambda: f"""
        WITH (SELECT id FROM soroban_contracts FINAL
              WHERE contract_id = '{CONTRACT}' LIMIT 1) AS cid
        SELECT ledger_sequence, transaction_id
        FROM soroban_invocations_appearances FINAL
        WHERE contract_id = cid
        ORDER BY ledger_sequence DESC, transaction_id DESC
        LIMIT 50
        FORMAT Null
        """,
    ),
    "E14": (
        "14_get_contracts_events.sql",
        lambda: f"""
        WITH (SELECT id FROM soroban_contracts FINAL
              WHERE contract_id = '{CONTRACT}' LIMIT 1) AS cid
        SELECT ledger_sequence, transaction_id, event_index
        FROM soroban_events FINAL
        WHERE contract_id = cid
        ORDER BY ledger_sequence DESC, transaction_id DESC, event_index DESC
        LIMIT 100
        FORMAT Null
        """,
    ),
    "E15": (
        "15_get_nfts_list.sql",
        lambda: """
        SELECT contract_id, token_id
        FROM nfts FINAL
        ORDER BY contract_id DESC, token_id DESC
        LIMIT 50
        FORMAT Null
        """,
    ),
    "E16": (
        "16_get_nfts_by_id.sql",
        lambda: f"""
        WITH (SELECT id FROM soroban_contracts FINAL
              WHERE contract_id = '{CONTRACT}' LIMIT 1) AS cid
        SELECT contract_id, token_id, current_owner_id
        FROM nfts FINAL
        WHERE contract_id = cid
        LIMIT 1
        FORMAT Null
        """,
    ),
    "E17": (
        "17_get_nfts_transfers.sql",
        lambda: f"""
        WITH (SELECT id FROM soroban_contracts FINAL
              WHERE contract_id = '{CONTRACT}' LIMIT 1) AS cid
        SELECT ledger_sequence, event_order
        FROM nft_ownership FINAL
        WHERE contract_id = cid
        ORDER BY ledger_sequence DESC, event_order DESC
        LIMIT 50
        FORMAT Null
        """,
    ),
    "E18": (
        "18_get_liquidity_pools_list.sql",
        lambda: """
        SELECT lp.pool_id, lp.fee_bps, lp.last_updated_ledger
        FROM liquidity_pools lp FINAL
        ORDER BY lp.last_updated_ledger DESC, lp.pool_id DESC
        LIMIT 50
        FORMAT Null
        """,
    ),
    "E19": (
        "19_get_liquidity_pools_by_id.sql",
        lambda: f"""
        SELECT lp.pool_id, lp.fee_bps, lp.last_updated_ledger
        FROM liquidity_pools AS lp FINAL
        WHERE lp.pool_id = unhex('{POOL}')
        LIMIT 1
        FORMAT Null
        """,
    ),
    "E20": (
        "20_get_liquidity_pools_transactions.sql",
        lambda: f"""
        SELECT DISTINCT ledger_sequence, transaction_id
        FROM operations_appearances
        WHERE pool_id = unhex('{POOL}')
          AND intDiv(ledger_sequence, 500000) = {PART}
        ORDER BY ledger_sequence DESC, transaction_id DESC
        LIMIT 50
        FORMAT Null
        """,
    ),
    "E21": (
        "21_get_liquidity_pools_chart.sql",
        lambda: f"""
        SELECT
            toStartOfInterval(l.closed_at, INTERVAL 3600 SECOND) AS bucket,
            argMax(s.tvl, s.ledger_sequence)
        FROM liquidity_pool_snapshots s FINAL
        JOIN ledgers l ON l.sequence = s.ledger_sequence
        WHERE s.pool_id = unhex('{POOL}')
          AND s.ledger_sequence >= {LEDGER - 1000}
          AND s.ledger_sequence <  {LEDGER}
        GROUP BY bucket ORDER BY bucket
        FORMAT Null
        """,
    ),
    "E22": (
        "22_get_search.sql",
        lambda: """
        SELECT count() FROM assets FINAL
        WHERE positionCaseInsensitiveUTF8(asset_code, 'USDC') > 0
        FORMAT Null
        """,
    ),
    "E23": (
        "23_get_liquidity_pools_participants.sql",
        lambda: f"""
        SELECT shares, account_id
        FROM lp_positions FINAL
        WHERE pool_id = unhex('{POOL}')
        ORDER BY shares DESC, account_id DESC
        LIMIT 50
        FORMAT Null
        """,
    ),
}


def measure(sql: str) -> float:
    t0 = time.monotonic()
    ch_query(sql, format="Null")
    return (time.monotonic() - t0) * 1000.0  # ms


def verdict(p95: float) -> str:
    if p95 < 100:
        return "FAST"
    if p95 < 500:
        return "OK"
    if p95 < 1500:
        return "SLOW"
    return "FAIL"


def main() -> int:
    TSV.parent.mkdir(parents=True, exist_ok=True)
    if not TSV.exists():
        TSV.write_text(
            "endpoint\tfile\tcold_first_ms\tp50_warm\tp95_warm\t"
            "p99_warm\tmax_ms\tmin_ms\tn_warm\tverdict\n"
        )

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    only = os.environ.get("SBE_F_ONLY", "").upper()
    started = time.monotonic()

    for ep, (sql_file, sql_factory) in ENDPOINTS.items():
        if only and ep not in {x.strip() for x in only.split(",")}:
            continue
        sql = sql_factory()
        n_runs = pilot if pilot > 0 else RUNS

        samples: list[float] = []
        cold_first = None
        for i in range(n_runs):
            try:
                ms = measure(sql)
            except RuntimeError as e:
                print(f"[{ep}] query error run={i}: {str(e)[:120]}",
                      file=sys.stderr)
                break
            samples.append(ms)
            if i == 0:
                cold_first = ms

        if len(samples) < WARM_DROP + 5:
            print(f"[{ep}] too few runs ({len(samples)}); skip", file=sys.stderr)
            continue

        warm = samples[WARM_DROP:]
        p50 = statistics.median(warm)
        p95 = (
            statistics.quantiles(warm, n=20, method="inclusive")[18]
            if len(warm) >= 20
            else max(warm)
        )
        p99 = (
            statistics.quantiles(warm, n=100, method="inclusive")[98]
            if len(warm) >= 100
            else max(warm)
        )
        mx = max(warm)
        mn = min(warm)
        v = verdict(p95)

        with TSV.open("a") as f:
            f.write(
                f"{ep}\t{sql_file}\t{cold_first:.2f}\t{p50:.2f}\t{p95:.2f}\t"
                f"{p99:.2f}\t{mx:.2f}\t{mn:.2f}\t{len(warm)}\t{v}\n"
            )

        elapsed_total = int(time.monotonic() - started)
        print(
            f"[{ep}] runs={len(samples)} cold={cold_first:.1f}ms "
            f"p50={p50:.1f} p95={p95:.1f} p99={p99:.1f} max={mx:.1f} "
            f"verdict={v} (cum elapsed={elapsed_total}s)",
            file=sys.stderr,
        )

    return 0


if __name__ == "__main__":
    sys.exit(main())
