#!/usr/bin/env python3
# scripts/0267/compare_e20_ch.py
#
# E20 (direct-CH) — validate operations_appearances pool attribution after the
# 0266 path-payment `pool_ids` backfill, against Horizon, per liquidity pool, by
# transaction hash-set. Recreates the (lost, never-committed) 0252 compare_e20.py
# but queries ClickHouse DIRECTLY instead of the API endpoint — so it needs NO
# redeploy and NO `api_reader` quota. Run it ON the prod box (ch-prod-01), where
# `docker exec app-clickhouse-1 clickhouse-client` reaches CH as the unquota'd
# default user.
#
# Per anchor pool, over the ledger window Horizon still retains, it reports:
#   recall    = |Horizon ∩ CH| / |Horizon|   — did the backfill MISS real pool txs?
#   precision = |Horizon ∩ CH| / |CH|         — did it OVER-ATTRIBUTE non-pool txs?
#
# Two chain realities are handled so they don't read as bugs:
#   * live-drift  — CH is frozen at W = max(sequence) while Horizon runs live
#                   ahead, so for an ACTIVE pool the recent Horizon page is ALL
#                   ledger > W. We JUMP to the <=W window with a constructed TOID
#                   cursor = (W+1)<<32 + order=desc (Horizon returns records
#                   strictly below it), skipping the drift in one request rather
#                   than paging tens of pages through it.
#   * retention   — Horizon prunes old history (404/410). The window is anchored
#                   at the oldest <=W tx Horizon still returns for the pool; pools
#                   Horizon has no data for are SKIPPED (counted, not failed). When
#                   the <=W set is truncated at --horizon-limit, the boundary ledger
#                   is excluded from both sides (Horizon may cut it off mid-ledger).
#
# Documented tolerances (NOT bugs — see lore 0252 / feedback_ch_horizon_semantic_diffs):
#   * CH backfill floor (~50,457,424): a pool whose window dips below it shows
#     floor artifacts, not misses — flagged `BELOW_FLOOR` via --floor.
#   * stellar.expert (full history) is intentionally NOT used here; Horizon's
#     retained window is the comparison surface (same as the original E20). For a
#     full-history precision audit of a single flagged pool, decode its txs by hand.
#
# Usage (on the prod box):
#   python3 compare_e20_ch.py [--anchors N | --pools-file FILE] [--floor L]
#                             [--horizon-limit H] [--sleep S] [--threshold T]
#   # TSV rows -> stdout ; progress, flags, summary -> stderr.
#   # keep results:  python3 compare_e20_ch.py --anchors 200 > e20.tsv
#
# stdlib only. Requires: `docker exec` to app-clickhouse-1, outbound HTTPS to horizon.

import argparse
import json
import subprocess
import sys
import time
import urllib.error
import urllib.request

CH_CONTAINER = "app-clickhouse-1"
HORIZON = "https://horizon.stellar.org"


def ch(query):
    """Run a query via `docker exec ... clickhouse-client`; return tab-split rows."""
    r = subprocess.run(
        ["docker", "exec", CH_CONTAINER, "clickhouse-client", "-q", query],
        capture_output=True,
        text=True,
    )
    if r.returncode != 0:
        raise RuntimeError(f"CH error: {r.stderr.strip()[:300]}")
    return [ln.split("\t") for ln in r.stdout.splitlines() if ln]


def http_json(url, retries=4):
    """GET JSON. 404/410 -> None (no data / retention). 429/5xx -> backoff."""
    for attempt in range(retries):
        try:
            req = urllib.request.Request(url, headers={"Accept": "application/json"})
            with urllib.request.urlopen(req, timeout=25) as resp:
                return json.load(resp)
        except urllib.error.HTTPError as e:
            if e.code in (404, 410):
                return None
            if e.code == 429 or 500 <= e.code < 600:
                time.sleep(1.5 * (attempt + 1))
                continue
            raise
        except (urllib.error.URLError, TimeoutError):
            time.sleep(1.5 * (attempt + 1))
    return None


def get_tip():
    return int(ch("SELECT max(sequence) FROM ledgers")[0][0])


def get_anchors(n):
    """Top-n active pools by pool-touching op count (path payments 2/13 + LP 22/23)."""
    q = (
        "SELECT lower(hex(p)) FROM operations_appearances ARRAY JOIN pool_ids AS p "
        "WHERE type IN (2, 13, 22, 23) GROUP BY p ORDER BY count() DESC LIMIT %d" % n
    )
    return [row[0] for row in ch(q)]


def horizon_pool_txs(pool_hex, want, tip, page_sleep):
    """Horizon txs for the pool at ledger <= tip, newest-first, up to `want`.
    JUMPS straight to the <=tip window with a constructed TOID cursor = (tip+1)<<32
    — the paging_token of the first slot of ledger tip+1 — plus order=desc, so
    Horizon returns records strictly below it (ledger <= tip), SKIPPING the live
    drift (> tip) in one request instead of paging tens of pages through it. CH is
    frozen at W=tip while Horizon runs live ahead; the hottest pools have thousands
    of drift txs, so paging back is hopeless — the cursor jump is exact and cheap.
    Returns [(hash, ledger)], or None if Horizon has no <=tip data for the pool
    (retention pruned it, pool created after W, or it never existed)."""
    # TOID = (ledger_seq << 32) | (tx_order << 12) | op_index ; start-of-ledger = seq<<32.
    cursor = (tip + 1) * 4294967296
    url = f"{HORIZON}/liquidity_pools/{pool_hex}/transactions?order=desc&limit=200&cursor={cursor}"
    out, saw_any = [], False
    for _ in range((want // 200) + 3):
        data = http_json(url)
        if not data or "_embedded" not in data:
            break
        recs = data["_embedded"]["records"]
        if not recs:
            break
        saw_any = True
        out.extend((r["hash"].lower(), int(r["ledger"])) for r in recs if int(r["ledger"]) <= tip)
        if len(out) >= want:
            break
        nxt = data.get("_links", {}).get("next", {}).get("href")
        if not nxt:
            break
        url = nxt
        time.sleep(page_sleep)
    if not saw_any:
        return None
    return out[:want]


def ch_pool_txs(pool_hex, lo, hi):
    """Distinct CH tx hashes touching the pool in ledger [lo, hi]."""
    q = (
        "SELECT DISTINCT lower(hex(t.hash)) "
        "FROM operations_appearances oa "
        "INNER JOIN transactions t "
        "  ON t.ledger_sequence = oa.ledger_sequence AND t.id = oa.transaction_id "
        "WHERE has(oa.pool_ids, toFixedString(unhex('%s'), 32)) "
        "  AND oa.ledger_sequence BETWEEN %d AND %d" % (pool_hex, lo, hi)
    )
    return {row[0] for row in ch(q)}


def main():
    ap = argparse.ArgumentParser(description="Direct-CH E20 pool-attribution check vs Horizon.")
    ap.add_argument("--anchors", type=int, default=50, help="top-N pools by op count")
    ap.add_argument("--pools-file", help="newline-separated pool hex ids; overrides --anchors")
    ap.add_argument("--floor", type=int, default=50_457_424, help="CH backfill floor ledger")
    ap.add_argument("--horizon-limit", type=int, default=200)
    ap.add_argument("--sleep", type=float, default=0.5, help="seconds between Horizon calls")
    ap.add_argument("--threshold", type=float, default=0.99, help="flag pools below this recall/precision")
    args = ap.parse_args()

    tip = get_tip()
    print(f"# W (tip) = {tip}   floor = {args.floor}", file=sys.stderr)

    if args.pools_file:
        with open(args.pools_file) as f:
            pools = [ln.strip().lower() for ln in f if ln.strip()]
    else:
        print(f"# generating top-{args.anchors} anchors from CH (full scan, ~10s) ...", file=sys.stderr)
        pools = get_anchors(args.anchors)
    print(f"# {len(pools)} anchor pools", file=sys.stderr)

    print("pool\twindow_lo\tn_horizon\tn_ch\tinter\trecall\tprecision\tnote")
    recalls, precisions, evaluated, skipped = [], [], 0, 0

    for i, p in enumerate(pools, 1):
        try:
            h = horizon_pool_txs(p, args.horizon_limit, tip, min(args.sleep, 0.3))
        except Exception as e:  # noqa: BLE001 — best-effort harness, keep going
            print(f"# ERR horizon {p}: {e}", file=sys.stderr)
            continue
        if h is None:
            skipped += 1
            print(f"# SKIP {p} — no Horizon data (retention / pool never existed)", file=sys.stderr)
            continue
        if not h:
            skipped += 1
            print(f"# SKIP {p} — no <=W txs (pool created after W, or retention pruned the <=W set)", file=sys.stderr)
            continue

        lo = min(led for _, led in h)
        # If Horizon truncated at the limit, its oldest ledger may be cut mid-ledger
        # -> exclude that boundary ledger from both sides so the window is apples-to-apples.
        win_lo = lo + 1 if len(h) >= args.horizon_limit else lo
        hset = {hsh for hsh, led in h if led >= win_lo}
        if not hset:
            skipped += 1
            continue

        try:
            cset = ch_pool_txs(p, win_lo, tip)
        except Exception as e:  # noqa: BLE001
            print(f"# ERR ch {p}: {e}", file=sys.stderr)
            continue

        inter = hset & cset
        recall = len(inter) / len(hset)
        precision = len(inter) / len(cset) if cset else 1.0
        note = "ok" if win_lo >= args.floor else "BELOW_FLOOR"
        evaluated += 1
        recalls.append(recall)
        precisions.append(precision)
        print(f"{p}\t{win_lo}\t{len(hset)}\t{len(cset)}\t{len(inter)}\t{recall:.4f}\t{precision:.4f}\t{note}")

        if recall < args.threshold or precision < args.threshold:
            print(
                f"#   FLAG {p} r={recall:.3f} p={precision:.3f} note={note} "
                f"horizon_only(recall_gap)={sorted(hset - cset)[:5]} "
                f"ch_only(over_attr)={sorted(cset - hset)[:5]}",
                file=sys.stderr,
            )
        if i % 10 == 0:
            print(f"# ... {i}/{len(pools)} (evaluated={evaluated} skipped={skipped})", file=sys.stderr)
        time.sleep(args.sleep)

    if evaluated:
        print(
            f"# DONE evaluated={evaluated} skipped(retention)={skipped} "
            f"mean_recall={sum(recalls) / evaluated:.4f} mean_precision={sum(precisions) / evaluated:.4f} "
            f"min_recall={min(recalls):.4f} min_precision={min(precisions):.4f}",
            file=sys.stderr,
        )
    else:
        print("# DONE — no pools evaluated (all skipped by retention?)", file=sys.stderr)


if __name__ == "__main__":
    main()
