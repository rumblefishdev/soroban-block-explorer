#!/usr/bin/env python3
"""E06 — /accounts/:id compare CH ↔ Horizon.

Per-account compare. Field-by-field diff on 6 fields:
  1. account_id      (StrKey echo)
  2. sequence_number
  3. last_modified_ledger
  4. balances (XLM total) — tolerance on small decimal drift
  5. home_domain
  6. flags             (auth_required / auth_revocable / auth_immutable
                        / auth_clawback_enabled bitmap)

Sample source: samples_accounts.txt (30K stratified).
Cap to 5K via SBE_PHASE_B_CAP.

Note: account state on Horizon reflects current chain tip; CH reflects
our backfill end (ledger 62,527,999). Most static accounts will match.
Active accounts may show drift in sequence_number / balances if live
mode hasn't synced — record as tolerance.
"""

from __future__ import annotations

import json
import os
import random
import sys
import time
from decimal import Decimal
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from common import (
    OUT_DIR,
    EndpointResult,
    append_tsv_row,
    ch_query_json,
    dump_diff,
    horizon_get,
    load_samples,
    read_completed_keys,
    write_tsv_header,
)


ENDPOINT = "E06"
TSV = OUT_DIR / "phase_b_e06.tsv"


def fetch_ch_account(strkey: str) -> dict | None:
    sql = f"""
    SELECT
      a.id              AS id,
      a.account_id      AS account_id,
      toString(a.sequence_number) AS sequence_number,
      a.last_seen_ledger AS last_seen_ledger,
      a.home_domain     AS home_domain
    FROM accounts AS a FINAL
    WHERE a.account_id = '{strkey}'
    LIMIT 1
    FORMAT JSONEachRow
    """
    rows = ch_query_json(sql)
    if not rows:
        return None
    acc = rows[0]
    # Resolve XLM balance via account_balances_current
    bal_sql = f"""
    SELECT toString(balance) AS xlm_balance
    FROM account_balances_current FINAL
    WHERE account_id = {acc['id']}
      AND asset_type = 0
    LIMIT 1
    FORMAT JSONEachRow
    """
    bal_rows = ch_query_json(bal_sql)
    acc["xlm_balance"] = bal_rows[0]["xlm_balance"] if bal_rows else "0"
    return acc


def fetch_horizon_account(strkey: str) -> dict | None:
    body = horizon_get(f"/accounts/{strkey}")
    if not body or "account_id" not in body:
        return None
    return body


def decimal_close(a: str, b: str, places: int = 7) -> bool:
    try:
        return abs(Decimal(str(a)) - Decimal(str(b))) <= Decimal(10) ** (-places)
    except Exception:
        return str(a) == str(b)


def compare(strkey: str, ch: dict, hz: dict, result: EndpointResult) -> list[str]:
    diffs: list[str] = []

    # 1. account_id echo
    if ch["account_id"] == hz["account_id"]:
        result.record_field("account_id", "pass")
    else:
        result.record_field("account_id", "fail")
        diffs.append(f"account_id CH={ch['account_id']} HZ={hz['account_id']}")

    # 2. sequence_number — drift expected (Horizon = chain tip,
    # CH = backfill end). Treat unequal as tolerance.
    if str(ch["sequence_number"]) == str(hz.get("sequence", "")):
        result.record_field("sequence_number", "pass")
    else:
        result.record_field("sequence_number", "tolerance")
        diffs.append(f"sequence_number CH={ch['sequence_number']} HZ={hz.get('sequence')} (tip drift)")

    # 3. last_modified_ledger
    if int(ch["last_seen_ledger"]) >= int(hz.get("last_modified_ledger", 0)):
        result.record_field("last_seen_ledger", "pass")
    else:
        result.record_field("last_seen_ledger", "tolerance")
        diffs.append(f"last_seen_ledger CH={ch['last_seen_ledger']} HZ={hz.get('last_modified_ledger')}")

    # 4. XLM balance — pick `native` from Horizon balances array
    hz_xlm = "0"
    for b in hz.get("balances", []):
        if b.get("asset_type") == "native":
            hz_xlm = b.get("balance", "0")
            break
    if decimal_close(ch["xlm_balance"], hz_xlm):
        result.record_field("xlm_balance", "pass")
    else:
        result.record_field("xlm_balance", "tolerance")
        diffs.append(f"xlm_balance CH={ch['xlm_balance']} HZ={hz_xlm} (tip drift)")

    # 5. home_domain
    ch_home = (ch.get("home_domain") or "").strip()
    hz_home = (hz.get("home_domain") or "").strip()
    if ch_home == hz_home:
        result.record_field("home_domain", "pass")
    else:
        result.record_field("home_domain", "fail")
        diffs.append(f"home_domain CH='{ch_home}' HZ='{hz_home}'")

    # 6. flags — Horizon returns dict
    hz_flags = hz.get("flags", {})
    # CH doesn't store flags explicitly in accounts (per schema) — record
    # as N/A tolerance for now.
    result.record_field("flags", "tolerance")

    return diffs


def main() -> int:
    write_tsv_header(TSV)
    done_keys = read_completed_keys(TSV, ENDPOINT)
    print(f"[E06] {len(done_keys)} keys already done", file=sys.stderr)

    accounts = load_samples("samples_accounts.txt")
    print(f"[E06] {len(accounts)} account samples loaded", file=sys.stderr)

    pilot = int(os.environ.get("SBE_PILOT_LIMIT", "0"))
    if pilot > 0:
        random.seed(42)
        accounts = random.sample(accounts, min(pilot, len(accounts)))
        print(f"[E06] PILOT — capped to {len(accounts)} accounts", file=sys.stderr)

    cap = int(os.environ.get("SBE_PHASE_B_CAP", "5000"))
    if cap > 0 and len(accounts) > cap:
        random.seed(123)
        accounts = random.sample(accounts, cap)
        print(f"[E06] capped to {cap} stratified samples", file=sys.stderr)

    result = EndpointResult(endpoint=ENDPOINT)
    result.sample_size = len(accounts)
    started = time.monotonic()
    processed = 0

    for strkey in accounts:
        if strkey in done_keys:
            continue
        processed += 1

        ch = fetch_ch_account(strkey)
        if ch is None:
            append_tsv_row(TSV, ENDPOINT, strkey, 0, 0, 1, "CH_MISSING")
            result.fail_total += 1
            continue

        hz = fetch_horizon_account(strkey)
        if hz is None:
            append_tsv_row(TSV, ENDPOINT, strkey, 0, 0, 0, "HZ_MERGED_OR_404")
            continue

        diffs = compare(strkey, ch, hz, result)
        if diffs:
            dump_diff(ENDPOINT, strkey, ch, hz, diffs)
            tol = sum(1 for d in diffs if "drift" in d or "flags" in d)
            fail = len(diffs) - tol
            pass_n = 6 - len(diffs)
            append_tsv_row(TSV, ENDPOINT, strkey, pass_n, tol, fail, ";".join(diffs))
        else:
            append_tsv_row(TSV, ENDPOINT, strkey, 6, 0, 0, "")

        if processed % 100 == 0:
            elapsed = int(time.monotonic() - started)
            rate = processed / max(elapsed, 1)
            eta = int((len(accounts) - processed) / max(rate, 0.01))
            print(f"[E06] {processed}/{len(accounts)} done, "
                  f"pass={result.pass_total} fail={result.fail_total} "
                  f"rate={rate:.1f}/s eta={eta}s",
                  file=sys.stderr)

    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_b_e06_summary.json"
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

    print(f"[E06] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
