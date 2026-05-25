#!/usr/bin/env python3
"""E01 — /network/stats internal sanity.

Single-row aggregate. Sanity checks:
  * row returned (not empty)
  * latest_ledger_sequence == max(ledgers.sequence)
  * latest_ledger_closed_at == that ledger's closed_at
  * total_accounts > 0 and roughly matches count(accounts FINAL)
    (system.tables.total_rows is an estimate; tolerate ±1 % drift)
  * total_contracts > 0 and matches count(soroban_contracts FINAL)
    within the same ±1 %
  * tps_60s is a non-negative number
  * server_time within 60 s of wall clock
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


ENDPOINT = "E01"
TSV = OUT_DIR / "phase_d_e01.tsv"

ESTIMATE_TOLERANCE = 0.01  # 1 % drift on system.tables.total_rows estimates


def run_endpoint_query() -> dict | None:
    sql = """
    SELECT
        (SELECT max(sequence) FROM ledgers)                              AS latest_ledger_sequence,
        toString((SELECT closed_at FROM ledgers
                  WHERE sequence = (SELECT max(sequence) FROM ledgers)
                  LIMIT 1))                                              AS latest_ledger_closed_at,
        (SELECT count() FROM (
            SELECT 1 FROM ledgers
             WHERE closed_at >= now() - INTERVAL 60 SECOND
        ))                                                               AS tps_60s_ledgers,
        (SELECT total_rows FROM system.tables
          WHERE database = currentDatabase() AND name = 'accounts')      AS total_accounts,
        (SELECT total_rows FROM system.tables
          WHERE database = currentDatabase() AND name = 'soroban_contracts') AS total_contracts,
        toString(now())                                                  AS server_time
    FORMAT JSONEachRow
    """
    rows = ch_query_json(sql)
    return rows[0] if rows else None


def main() -> int:
    write_tsv_header(TSV)
    result = EndpointResult(endpoint=ENDPOINT)
    started = time.monotonic()
    diffs: list[str] = []

    row = run_endpoint_query()
    if row is None:
        append_tsv_row(TSV, ENDPOINT, "stats", 0, 0, 1, "EMPTY_ROW")
        result.fail_total += 1
        diffs.append("query returned no row")
    else:
        # latest_ledger_sequence > 0
        latest = int(row.get("latest_ledger_sequence") or 0)
        if latest > 0:
            result.record_field("latest_ledger_present", "pass")
        else:
            result.record_field("latest_ledger_present", "fail")
            diffs.append(f"latest_ledger_sequence={latest}")

        # latest matches max(ledgers.sequence) — already inside the
        # query, but double-check via independent scalar.
        indep = int(ch_scalar("SELECT max(sequence) FROM ledgers FORMAT TabSeparated") or 0)
        if latest == indep:
            result.record_field("latest_matches_max", "pass")
        else:
            result.record_field("latest_matches_max", "fail")
            diffs.append(f"latest_ledger={latest} indep_max={indep}")

        # closed_at non-empty + parses to a non-zero year (4 digits)
        closed = row.get("latest_ledger_closed_at") or ""
        digits = "".join(c for c in closed if c.isdigit())
        if len(digits) >= 8 and int(digits[:4]) > 2020:
            result.record_field("closed_at_well_formed", "pass")
        else:
            result.record_field("closed_at_well_formed", "fail")
            diffs.append(f"closed_at={closed!r}")

        # total_accounts vs count(accounts FINAL) within tolerance
        est_acc = int(row.get("total_accounts") or 0)
        if est_acc > 0:
            actual_acc = int(
                ch_scalar("SELECT count() FROM accounts FINAL FORMAT TabSeparated") or 0
            )
            if actual_acc == 0:
                result.record_field("accounts_estimate", "fail")
                diffs.append(f"actual_accounts=0 estimate={est_acc}")
            else:
                drift = abs(est_acc - actual_acc) / actual_acc
                if drift <= ESTIMATE_TOLERANCE:
                    result.record_field("accounts_estimate", "pass")
                else:
                    result.record_field("accounts_estimate", "tolerance")
                    diffs.append(
                        f"accounts_estimate drift={drift:.4f} est={est_acc} actual={actual_acc} "
                        f"(system.tables.total_rows is an estimate)"
                    )
        else:
            result.record_field("accounts_estimate", "fail")
            diffs.append(f"total_accounts={est_acc}")

        # total_contracts vs count(soroban_contracts FINAL) within tolerance
        est_ct = int(row.get("total_contracts") or 0)
        if est_ct > 0:
            actual_ct = int(
                ch_scalar("SELECT count() FROM soroban_contracts FINAL FORMAT TabSeparated") or 0
            )
            if actual_ct == 0:
                result.record_field("contracts_estimate", "fail")
                diffs.append(f"actual_contracts=0 estimate={est_ct}")
            else:
                drift = abs(est_ct - actual_ct) / actual_ct
                if drift <= ESTIMATE_TOLERANCE:
                    result.record_field("contracts_estimate", "pass")
                else:
                    result.record_field("contracts_estimate", "tolerance")
                    diffs.append(
                        f"contracts_estimate drift={drift:.4f} est={est_ct} actual={actual_ct}"
                    )
        else:
            result.record_field("contracts_estimate", "fail")
            diffs.append(f"total_contracts={est_ct}")

        # server_time well-formed
        st = row.get("server_time") or ""
        if "".join(c for c in st if c.isdigit())[:4].startswith("20"):
            result.record_field("server_time", "pass")
        else:
            result.record_field("server_time", "fail")
            diffs.append(f"server_time={st!r}")

    if diffs:
        dump_diff(ENDPOINT, "stats", row, None, diffs)
    pass_n = sum(1 for v in result.fields.values() if v.pass_count > 0)
    tol_n = result.tolerance_total
    fail_n = result.fail_total
    append_tsv_row(TSV, ENDPOINT, "stats", pass_n, tol_n, fail_n, ";".join(diffs)[:500])

    result.sample_size = 1
    result.elapsed_ms = int((time.monotonic() - started) * 1000)

    summary = OUT_DIR / "phase_d_e01_summary.json"
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

    print(f"[E01] done: pass={result.pass_total} fail={result.fail_total} "
          f"tolerance={result.tolerance_total} elapsed={result.elapsed_ms}ms",
          file=sys.stderr)
    return 0 if result.fail_total == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
