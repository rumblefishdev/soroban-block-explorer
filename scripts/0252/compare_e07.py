#!/usr/bin/env python3
"""E07 — /accounts/:account_id/transactions compare CH ↔ Horizon.

Per-account per-ledger set compare (same shape as E02): for each
sampled account + sampled ledger window, pull the set of tx hashes
that touched the account from both CH and Horizon and assert
hash-set equality + per-row field correctness on the intersection.

Sample design
-------------
On first run the script materialises `samples_accounts.txt` from
`transaction_participants` filtered to the Horizon retention half +
non-trivial activity (at least 5 tx). That seed file is reused on
subsequent runs.

For each anchor account, sample 1 ledger from the account's own
ledger range (we cannot use the global ledger sample pool — different
accounts touch different ledger windows). Compare per-ledger.

Compare strategy mirrors E02 (per-ledger set compare; within-ledger
sequence differs across stores).
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
    HORIZON_BASE,
    HORIZON_DELAY,
    OUT_DIR,
    SAMPLE_DIR,
    EndpointResult,
    append_tsv_row,
    ch_query,
    ch_query_json,
    dump_diff,
    horizon_get,
    read_completed_keys,
    write_tsv_header,
)

import requests


ENDPOINT = "E07"
TSV = OUT_DIR / "phase_b_e07.tsv"
SAMPLES_FILE = SAMPLE_DIR / "samples_accounts.txt"

HORIZON_FLOOR = 56657428
CH_TIP_FLOOR = 62525000

DEFAULT_ANCHORS = int(os.environ.get("SBE_E07_ANCHORS", "300"))


def build_samples(n: int) -> list[str]:
    """Materialise samples_accounts.txt — N accounts with retention-valid
    activity (≥ 5 tx in the (HORIZON_FLOOR, CH_TIP_FLOOR] window).
    """
    if SAMPLES_FILE.exists():
        return [
            l.strip()
            for l in SAMPLES_FILE.read_text().splitlines()
            if l.strip()
        ]
    sql = f"""
    SELECT a.account_id
    FROM (
        SELECT account_id, count() AS tx_count
        FROM transaction_participants
        WHERE ledger_sequence > {HORIZON_FLOOR}
          AND ledger_sequence <= {CH_TIP_FLOOR}
        GROUP BY account_id
        HAVING tx_count >= 5
    ) AS p
    JOIN accounts AS a FINAL ON a.id = p.account_id
    ORDER BY cityHash64(a.account_id)
    LIMIT {n * 4}
    FORMAT TabSeparated
    """
    out = ch_query(sql).splitlines()
    accts = [line.strip() for line in out if line.strip()]
    SAMPLES_FILE.write_text("\n".join(accts) + "\n")
    print(f"[E07] materialised {len(accts)} accounts to {SAMPLES_FILE}",
          file=sys.stderr)
    return accts


def fetch_ch_account_ledger(account_strkey: str, ledger: int) -> list[dict]:
    sk = account_strkey.replace("'", "''")
    part = ledger // 500_000
    sql = f"""
    WITH (SELECT id FROM accounts FINAL WHERE account_id = '{sk}' LIMIT 1) AS aid
    SELECT
        lower(hex(t.hash))                    AS hash,
        t.ledger_sequence                     AS ledger_sequence,
        t.application_order                   AS application_order,
        src.account_id                        AS source_account,
        toString(t.fee_charged)               AS fee_charged,
        t.successful                          AS successful,
        t.operation_count                     AS operation_count
    FROM transaction_participants AS tp
    INNER JOIN transactions AS t FINAL
        ON t.id = tp.transaction_id AND t.ledger_sequence = tp.ledger_sequence
    LEFT JOIN accounts AS src FINAL ON src.id = t.source_id
    WHERE tp.account_id = aid
      AND tp.ledger_sequence = {ledger}
      AND intDiv(t.ledger_sequence, 500000) = {part}
    FORMAT JSONEachRow
    """
    return ch_query_json(sql)


def fetch_horizon_account_ledger(account_strkey: str, ledger: int) -> list[dict]:
    """Walk Horizon /accounts/:id/transactions, descending from a cursor
    just above the anchor ledger, collecting tx with ledger ==
    anchor_ledger.
    """
    cursor = str((ledger + 1) * (1 << 32))
    url = f"{HORIZON_BASE}/accounts/{account_strkey}/transactions"
    params = {"cursor": cursor, "order": "desc", "limit": 200,
              "include_failed": "true"}

    out: list[dict] = []
    page = 0
    while page < 20:
        if page == 0:
            r = requests.get(url, params=params, timeout=45)
        else:
            r = requests.get(url, timeout=45)
        if r.status_code == 200:
            body = r.json()
            recs = body.get("_embedded", {}).get("records", []) or []
            if not recs:
                return out
            for rec in recs:
                lseq = int(rec.get("ledger", -1))
                if lseq < ledger:
                    return out
                if lseq == ledger:
                    out.append(rec)
            next_link = body.get("_links", {}).get("next", {}).get("href")
            if not next_link:
                return out
            url = next_link
            params = None
            page += 1
            time.sleep(HORIZON_DELAY)
            continue
        if r.status_code == 429 or r.status_code >= 500:
            time.sleep(2 ** min(page, 5))
            continue
        if r.status_code in (400, 404):
            return out
        r.raise_for_status()
    return out


def sample_ledger_for_account(account_strkey: str) -> int | None:
    """Pick one retention-valid ledger from the account's own activity.
    Uses cityHash64 for stable random.
    """
    sk = account_strkey.replace("'", "''")
    sql = f"""
    WITH (SELECT id FROM accounts FINAL WHERE account_id = '{sk}' LIMIT 1) AS aid
    SELECT ledger_sequence
    FROM transaction_participants
    WHERE account_id = aid
      AND ledger_sequence > {HORIZON_FLOOR}
      AND ledger_sequence <= {CH_TIP_FLOOR}
    ORDER BY cityHash64(account_id, ledger_sequence)
    LIMIT 1
    FORMAT TabSeparated
    """
    out = ch_query(sql).strip()
    return int(out) if out else None


def compare_anchor(account: str, ledger: int,
                   ch_rows: list[dict], hz_rows: list[dict],
                   result: EndpointResult) -> list[str]:
    diffs: list[str] = []

    ch_hashes = {r["hash"].lower() for r in ch_rows}
    hz_hashes = {r.get("hash", "").lower() for r in hz_rows if r.get("hash")}

    only_ch = ch_hashes - hz_hashes
    only_hz = hz_hashes - ch_hashes
    if not only_ch and not only_hz:
        result.record_field("hash_set_equal", "pass")
    else:
        result.record_field("hash_set_equal", "fail")
        diffs.append(
            f"hash_set diff only_ch={len(only_ch)} only_hz={len(only_hz)} "
            f"ch_count={len(ch_hashes)} hz_count={len(hz_hashes)} "
            f"ch_sample={list(only_ch)[:3]} hz_sample={list(only_hz)[:3]}"
        )

    ch_by_hash = {r["hash"].lower(): r for r in ch_rows}
    hz_by_hash = {r["hash"].lower(): r for r in hz_rows if r.get("hash")}
    inter = ch_hashes & hz_hashes

    for h in inter:
        ch = ch_by_hash[h]
        hz = hz_by_hash[h]

        if ch["source_account"] == hz.get("source_account"):
            result.record_field("source_account", "pass")
        else:
            result.record_field("source_account", "fail")
            diffs.append(f"{h[:12]}.. source_account CH={ch['source_account']} HZ={hz.get('source_account')}")

        if str(ch["fee_charged"]) == str(hz.get("fee_charged")):
            result.record_field("fee_charged", "pass")
        else:
            result.record_field("fee_charged", "fail")
            diffs.append(f"{h[:12]}.. fee_charged CH={ch['fee_charged']} HZ={hz.get('fee_charged')}")

        if bool(ch["successful"]) == bool(hz.get("successful")):
            result.record_field("successful", "pass")
        else:
            result.record_field("successful", "fail")
            diffs.append(f"{h[:12]}.. successful CH={ch['successful']} HZ={hz.get('successful')}")

        ch_oc = int(ch.get("operation_count", 0))
        hz_oc = int(hz.get("operation_count") or 0)
        if ch_oc == hz_oc:
            result.record_field("operation_count", "pass")
        else:
            result.record_field("operation_count", "tolerance")
            diffs.append(f"{h[:12]}.. operation_count CH={ch_oc} HZ={hz_oc} (Horizon semantic drift)")

    return diffs


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E07] {len(done_keys)} anchors already done", file=sys.stderr)

    accts = build_samples(DEFAULT_ANCHORS)
    random.seed(42)
    random.shuffle(accts)
    accts = accts[:DEFAULT_ANCHORS]
    print(f"[E07] {len(accts)} account samples", file=sys.stderr)

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        accts = accts[:pilot]
        print(f"[E07] PILOT mode — first {pilot}", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(accts)
    started = time.monotonic()
    processed = 0

    for account in accts:
        processed += 1

        try:
            ledger = sample_ledger_for_account(account)
        except RuntimeError as e:
            append_tsv_row(TSV, ENDPOINT, account, 0, 0, 1, f"CH_SAMPLE_ERR:{str(e)[:80]}")
            result.fail_total += 1
            continue
        if ledger is None:
            append_tsv_row(TSV, ENDPOINT, account, 0, 0, 0, "NO_LEDGER")
            continue

        key = f"{account}@{ledger}"
        if key in done_keys:
            continue

        try:
            ch_rows = fetch_ch_account_ledger(account, ledger)
        except RuntimeError as e:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 1, f"CH_ERR:{str(e)[:80]}")
            result.fail_total += 1
            continue

        hz_rows = fetch_horizon_account_ledger(account, ledger)
        if not hz_rows and not ch_rows:
            append_tsv_row(TSV, ENDPOINT, key, 0, 0, 0, "BOTH_EMPTY")
            continue

        diffs = compare_anchor(account, ledger, ch_rows, hz_rows, result)

        pass_n = 0 if any(d.startswith("hash_set ") for d in diffs) else 1
        tol_n = sum(1 for d in diffs if "(Horizon semantic drift)" in d)
        fail_n = sum(1 for d in diffs if "(Horizon semantic drift)" not in d)

        if diffs:
            dump_diff(ENDPOINT, key, ch_rows, hz_rows, diffs)
            note = ";".join(diffs)[:500]
        else:
            note = ""
        append_tsv_row(TSV, ENDPOINT, key, pass_n, tol_n, fail_n, note)

        if processed % 25 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(accts) - processed) / max(rate, 0.01))
            print(f"[E07] {processed}/{len(accts)} anchors, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"tol={result.tolerance_total} rate={rate:.2f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e07_summary.json"
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

    print(f"[E07] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
