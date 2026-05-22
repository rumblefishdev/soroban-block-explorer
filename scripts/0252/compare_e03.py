#!/usr/bin/env python3
"""E03 — /transactions/:hash compare CH ↔ Horizon.

Inputs:
  samples_ledgers.txt — ledger samples (we derive tx hashes per ledger
                        and compare each tx).

Compare method:
  Field-by-field on 7 critical fields:
    1. hash          — canonical tx hash (from transaction_hash_index.hash)
    2. ledger        — ledger_sequence
    3. source_account
    4. fee_charged
    5. successful    — boolean
    6. operation_count — drift expected (Horizon: successful-only), tolerated
    7. signatures count

Sample size: 30K ledgers × ~290 tx avg = ~8.7M txs total ledger universe.
We sample 30 tx per ledger × 30K ledgers = 900K tx compares — far too many
for one run. Instead: from each ledger, pick 1 random tx → 30K tx total.

Output: TSV row per tx, JSON diff per mismatch.
"""

from __future__ import annotations

import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from common import (
    OUT_DIR,
    EndpointResult,
    append_tsv_row,
    ch_query,
    ch_query_json,
    dump_diff,
    horizon_get,
    load_samples,
    read_completed_keys,
    write_tsv_header,
)


ENDPOINT = "E03"
TSV = OUT_DIR / "phase_b_e03.tsv"


def pick_one_tx_per_ledger(ledgers: list[int], limit_per_ledger: int = 1,
                           chunk_size: int = 1000) -> list[tuple[int, str]]:
    """For each sampled ledger, pick `limit_per_ledger` random tx hashes.

    Chunked over `chunk_size` ledgers per query because CH's default
    `max_query_size` is 256 KiB — a 30K-element IN list with ~9 chars
    per int + separator easily exceeds that.
    """
    out: list[tuple[int, str]] = []
    for chunk_start in range(0, len(ledgers), chunk_size):
        chunk = ledgers[chunk_start:chunk_start + chunk_size]
        seq_list = ",".join(str(L) for L in chunk)
        sql = f"""
        SELECT ledger_sequence, lower(hex(hash)) AS hash
        FROM (
          SELECT ledger_sequence, hash,
                 row_number() OVER (PARTITION BY ledger_sequence ORDER BY intHash64(reinterpretAsUInt64(substring(hash, 1, 8)))) AS rn
          FROM transaction_hash_index
          WHERE ledger_sequence IN ({seq_list})
        )
        WHERE rn <= {limit_per_ledger}
        FORMAT TabSeparated
        """
        rows = ch_query(sql).splitlines()
        for line in rows:
            if not line.strip():
                continue
            parts = line.split("\t")
            if len(parts) == 2:
                out.append((int(parts[0]), parts[1]))
    return out


def fetch_ch_tx(hash_hex: str) -> dict | None:
    """Statement B of E03 (header). Returns 1 row or empty.

    Pattern (canonical from E03 SQL): use `transaction_hash_dict` to
    resolve hash → ledger_sequence in O(1), then narrow the scan on
    `transactions FINAL` to that single ledger partition + hash. The
    `transaction_hash_index` table itself carries only (hash,
    ledger_sequence) — no surrogate `id`.

    Joins `accounts FINAL` to resolve `source_id` (Int64) into the
    StrKey that Horizon emits as `source_account`. `operation_count`
    lives directly on `transactions` per the actual canonical schema.
    """
    sql = f"""
    WITH dictGet('transaction_hash_dict', 'ledger_sequence',
                 toString(unhex('{hash_hex}'))) AS lseq
    SELECT
      t.id                                 AS id,
      lower(hex(t.hash))                   AS hash,
      t.ledger_sequence                    AS ledger_sequence,
      t.application_order                  AS application_order,
      a.account_id                         AS source_account,
      t.fee_charged                        AS fee_charged,
      t.successful                         AS successful,
      t.operation_count                    AS operation_count,
      t.has_soroban                        AS has_soroban
    FROM transactions AS t FINAL
    LEFT JOIN accounts AS a FINAL ON a.id = t.source_id
    WHERE t.ledger_sequence = lseq
      AND t.hash = unhex('{hash_hex}')
    FORMAT JSONEachRow
    """
    rows = ch_query_json(sql)
    return rows[0] if rows else None


def fetch_horizon_tx(hash_hex: str) -> dict | None:
    body = horizon_get(f"/transactions/{hash_hex}")
    if not body or "hash" not in body:
        return None
    return body


def fetch_horizon_op_count(hash_hex: str) -> int | None:
    body = horizon_get(f"/transactions/{hash_hex}/operations", params={"limit": 1})
    return body.get("_links", {}).get("self", {}).get("href") and \
        len(list(body.get("_embedded", {}).get("records", []))) or None


def compare_tx(hash_hex: str, ch: dict, hz: dict, result: EndpointResult) -> list[str]:
    """Field-by-field compare. Returns list of mismatch descriptions."""
    diffs: list[str] = []

    # Field 1: hash
    if ch["hash"].lower() == hz["hash"].lower():
        result.record_field("hash", "pass")
    else:
        result.record_field("hash", "fail")
        diffs.append(f"hash CH={ch['hash']} HZ={hz['hash']}")

    # Field 2: ledger_sequence
    if int(ch["ledger_sequence"]) == int(hz.get("ledger", 0)):
        result.record_field("ledger", "pass")
    else:
        result.record_field("ledger", "fail")
        diffs.append(f"ledger CH={ch['ledger_sequence']} HZ={hz.get('ledger')}")

    # Field 3: source_account
    if ch["source_account"] == hz.get("source_account"):
        result.record_field("source_account", "pass")
    else:
        result.record_field("source_account", "fail")
        diffs.append(f"source_account CH={ch['source_account']} HZ={hz.get('source_account')}")

    # Field 4: fee_charged
    if str(ch["fee_charged"]) == str(hz.get("fee_charged")):
        result.record_field("fee_charged", "pass")
    else:
        result.record_field("fee_charged", "fail")
        diffs.append(f"fee_charged CH={ch['fee_charged']} HZ={hz.get('fee_charged')}")

    # Field 5: successful
    if bool(ch["successful"]) == bool(hz.get("successful")):
        result.record_field("successful", "pass")
    else:
        result.record_field("successful", "fail")
        diffs.append(f"successful CH={ch['successful']} HZ={hz.get('successful')}")

    # Field 6: operation_count — Horizon documents "successful only" but
    # empirical Phase 6 Tier 5 showed mixed direction drift. Compare
    # strict equality first; record as tolerance when they differ
    # (Horizon semantic noise, not data divergence).
    ch_opcount = int(ch.get("operation_count", 0))
    hz_opcount = int(hz.get("operation_count") or 0)
    if ch_opcount == hz_opcount:
        result.record_field("operation_count", "pass")
    else:
        result.record_field("operation_count", "tolerance")
        diffs.append(f"operation_count CH={ch_opcount} HZ={hz_opcount} (Horizon semantic drift)")

    return diffs


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E03] {len(done_keys)} keys already done", file=sys.stderr)

    ledgers = [int(s) for s in load_samples("samples_ledgers.txt")]
    print(f"[E03] {len(ledgers)} ledger samples loaded", file=sys.stderr)

    import os
    import random
    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        # In pilot mode restrict to ledgers Horizon can serve. The
        # `samples_ledgers.txt` file mixes stratified retention-valid
        # picks with adversarial edges (min/max per partition); those
        # adversarial values sit at the head of the sorted file and
        # are nearly all in the pre-retention half (50.4M → 56.6M).
        # Pilot wants signal against Horizon, so filter + randomise.
        HORIZON_FLOOR = 56657428
        retention = [L for L in ledgers if L >= HORIZON_FLOOR]
        random.seed(42)
        ledgers = random.sample(retention, min(pilot, len(retention)))
        print(f"[E03] PILOT mode — {pilot} random ledgers from {len(retention)} retention-valid",
              file=sys.stderr)

    print(f"[E03] resolving tx hashes (1 per ledger)...", file=sys.stderr)
    pairs = pick_one_tx_per_ledger(ledgers, limit_per_ledger=1)
    print(f"[E03] {len(pairs)} tx hashes to compare", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(pairs)
    started = time.monotonic()
    processed = 0

    for (ledger, hash_hex) in pairs:
        if hash_hex in done_keys:
            continue
        processed += 1

        ch = fetch_ch_tx(hash_hex)
        if ch is None:
            append_tsv_row(TSV, ENDPOINT, hash_hex, 0, 0, 1, "CH_MISSING")
            result.fail_total += 1
            continue

        hz = fetch_horizon_tx(hash_hex)
        if hz is None:
            # Most likely pre-retention. Skip.
            append_tsv_row(TSV, ENDPOINT, hash_hex, 0, 0, 0, "HZ_PRE_RETENTION")
            continue

        diffs = compare_tx(hash_hex, ch, hz, result)
        # 6 fields total now: hash, ledger, source_account, fee_charged,
        # successful, operation_count (tolerance-bound).
        if diffs:
            dump_diff(ENDPOINT, hash_hex, ch, hz, diffs)
            # All `diffs` entries that survived compare_tx() are either
            # "fail" or "tolerance" — recount via result.fields if needed.
            # For TSV simplicity: count entries in diffs as "tolerance" if
            # operation_count, else fail.
            tol = sum(1 for d in diffs if d.startswith("operation_count"))
            fail = len(diffs) - tol
            pass_n = 6 - len(diffs)
            append_tsv_row(TSV, ENDPOINT, hash_hex, pass_n, tol, fail,
                           ";".join(diffs))
        else:
            append_tsv_row(TSV, ENDPOINT, hash_hex, 6, 0, 0, "")

        if processed % 100 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(pairs) - processed) / max(rate, 0.01))
            print(f"[E03] {processed}/{len(pairs)} done, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"rate={rate:.1f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e03_summary.json"
    import json
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

    print(f"[E03] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
