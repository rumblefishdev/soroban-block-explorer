"""Common helpers for Phase D Group C — internal-consistency endpoint checks.

Group C endpoints have no external comparator (no Horizon equivalent for
network stats, search, NFT internals, LP chart, etc.) so the compare
shape is:
  * row exists where the query promises one
  * FKs (transaction_id, contract_id, account_id, issuer_id, pool_id)
    resolve to real rows in their target tables
  * paginated cursors advance monotonically (no duplicates, no skips,
    DESC ordering claim holds within the sample)
  * counts / sums internally consistent (e.g. E01 latest_ledger ==
    max(ledgers.sequence))

Each `phase_d_eNN.py` script imports from this module to:
  - sample inputs from already-collected sample pools (or the live tables)
  - run small CH queries to assert the invariants
  - write a TSV row per check + a JSON summary, identical schema to
    Phase B (re-uses `phase_b_e<NN>` flow via `common.append_tsv_row`).
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from common import ch_query, ch_query_json  # noqa: F401  (re-exported)


def ch_scalar(sql: str) -> str:
    """Run a CH query expected to return one scalar (a single row, single
    column). Returns the raw stringified value (caller casts).
    """
    out = ch_query(sql, format="TabSeparated").strip()
    return out


def ch_count(sql_where: str, table: str = "ledgers") -> int:
    out = ch_scalar(f"SELECT count() FROM {table} WHERE {sql_where} FORMAT TabSeparated")
    return int(out) if out else 0


def assert_monotonic_desc(values: list, label: str = "") -> bool:
    """Verify a list is strictly monotonic DESC (no equals, no inversions).
    Returns True on pass, False on fail. Empty/single-element lists are pass.
    """
    if len(values) < 2:
        return True
    for a, b in zip(values, values[1:]):
        if a < b:
            return False
        if a == b:
            return False
    return True


def assert_monotonic_desc_loose(values: list) -> bool:
    """Same as `_desc` but allows equals — for tuple cursors where the
    leading key alone is not unique.
    """
    if len(values) < 2:
        return True
    for a, b in zip(values, values[1:]):
        if a < b:
            return False
    return True
