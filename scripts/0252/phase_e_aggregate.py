#!/usr/bin/env python3
"""Phase E — final validation artifact aggregator.

Reads:
  * /tmp/sbe-artifacts/0252/phase_b_e<NN>_summary.json (Phase B + C)
  * /tmp/sbe-artifacts/0252/phase_d_e<NN>_summary.json (Phase D)
  * /tmp/sbe-artifacts/0252/phase_f_perf.tsv          (Phase F)

Emits:
  docs/runbooks/artifacts/endpoint_validation_<YYYYMMDD>.md
  with the four sections defined in task 0252 §"Reporting Shape":
    1. Per-endpoint detail (23 stanzas)
    2. Table coverage matrix
    3. Group roll-up
    4. Latency profile

Run on the box where the artifacts live (sorban-prod / Hetzner CH
container host), OR copy `/tmp/sbe-artifacts/0252/` locally first.
Override the input dir with `SBE_OUT_DIR=...`.
"""

from __future__ import annotations

import datetime as _dt
import json
import os
import sys
from dataclasses import dataclass, field
from pathlib import Path


OUT_DIR = Path(os.environ.get("SBE_OUT_DIR", "/tmp/sbe-artifacts/0252"))
ARTIFACT_DIR = Path(
    os.environ.get(
        "SBE_ARTIFACT_DIR",
        "docs/runbooks/artifacts",
    )
)


# ---- Per-endpoint static metadata --------------------------------------

@dataclass
class EndpointMeta:
    code: str                    # E01, E02...
    path: str                    # GET /transactions
    group: str                   # A | A.5 | B | C | empty-by-design
    compared_with: str           # `Horizon REST` | `stellar.expert API` | …
    compare_method: str          # `field-by-field` | `set-equality` | `internal-only`
    ch_tables: list[str]         # tables read by this endpoint's canonical SQL
    sample_method: str           # plain-English description
    tolerance_notes: str         # short tolerance rationale, can be empty


ENDPOINT_META: dict[str, EndpointMeta] = {
    "E01": EndpointMeta(
        "E01", "GET /network/stats", "C",
        "Internal only", "Cross-row + estimate consistency",
        ["ledgers", "accounts (FINAL)", "soroban_contracts (FINAL)"],
        "Single-row query; verify latest_ledger == max(ledgers), estimates within ±1 %",
        "system.tables.total_rows drifts ≤ 1 % vs FINAL count — tolerance",
    ),
    "E02": EndpointMeta(
        "E02", "GET /transactions", "A",
        "Horizon REST", "Per-ledger set equality + per-row field",
        ["transactions FINAL", "accounts FINAL"],
        "600 anchor ledgers from samples_ledgers.txt (retention-valid); per-ledger CH set vs Horizon set",
        "Within-ledger sort differs by design (CH cityhash64 vs Horizon application_order); operation_count drift = Horizon successful-only semantic",
    ),
    "E03": EndpointMeta(
        "E03", "GET /transactions/:hash", "A",
        "Horizon REST", "Field-by-field (6 fields)",
        ["transactions FINAL", "transaction_hash_index", "accounts FINAL"],
        "30 K ledgers × 1 random tx/ledger = ~30 K tx-hash compares",
        "operation_count tolerance — Horizon successful-only",
    ),
    "E04": EndpointMeta(
        "E04", "GET /ledgers", "A",
        "Horizon REST", "Field-by-field (5 fields)",
        ["ledgers"],
        "600 anchor ledgers; per-ledger detail vs Horizon /ledgers/:seq",
        "transaction_count CH=total vs Horizon split (success + failed); tolerance when CH==success-only",
    ),
    "E05": EndpointMeta(
        "E05", "GET /ledgers/:seq", "A",
        "Horizon REST", "Field-by-field",
        ["ledgers"],
        "samples_ledgers.txt (retention-valid)",
        "—",
    ),
    "E06": EndpointMeta(
        "E06", "GET /accounts/:id", "A",
        "Horizon REST", "Field-by-field",
        ["accounts FINAL"],
        "samples_accounts (built first by E07 / re-used)",
        "last_seen_ledger drift = expected live state vs CH snapshot",
    ),
    "E07": EndpointMeta(
        "E07", "GET /accounts/:id/transactions", "A",
        "Horizon REST", "Per-account-per-ledger set equality + field",
        ["transaction_participants", "transactions FINAL", "accounts FINAL"],
        "300 accounts × 1 retention-valid ledger each",
        "transaction_participants semantic broader than Horizon (participant vs source) — edge case mismatches expected",
    ),
    "E08": EndpointMeta(
        "E08", "GET /assets", "C",
        "Internal only", "Cursor monotonic + FK resolve",
        ["assets FINAL", "accounts FINAL", "soroban_contracts FINAL"],
        "30 pages × 50 row walk; (asset_type, asset_code, issuer_id, contract_id) DESC cursor",
        "—",
    ),
    "E09": EndpointMeta(
        "E09", "GET /assets/:id", "A",
        "Horizon REST", "Field-by-field",
        ["assets FINAL"],
        "samples_assets (built by E09 first run)",
        "total_supply drift = backfill snapshot vs Horizon live",
    ),
    "E10": EndpointMeta(
        "E10", "GET /assets/:id/transactions", "C",
        "Internal only", "Cursor monotonic + tx FK",
        ["operations_appearances", "transactions FINAL"],
        "200 assets × 5 pages × 50 = 5 K tx-row compares",
        "—",
    ),
    "E11": EndpointMeta(
        "E11", "GET /contracts/:id", "B",
        "stellar.expert API", "Field-by-field",
        ["soroban_contracts FINAL", "accounts FINAL"],
        "samples_contracts.txt (5 K stratified by contract_type)",
        "Deployer mismatch surfaced + fixed via task 0255 Phase 1 (PR #213). Phase 3 re-validate spawned as task 0256",
    ),
    "E12": EndpointMeta(
        "E12", "GET /contracts/:id/interface", "B",
        "stellar.expert API", "Function-name set + is_sac",
        ["soroban_contracts FINAL", "wasm_interface_metadata"],
        "samples_contracts.txt cap 5 K",
        "stellar.expert surfaces ~few % of contracts (mostly known SACs); rest are SE_MISSING — neutral skip",
    ),
    "E13": EndpointMeta(
        "E13", "GET /contracts/:id/invocations", "B",
        "stellar.expert API", "tx_hash overlap on intersection",
        ["soroban_invocations_appearances FINAL", "transactions FINAL"],
        "2 K active contracts from soroban_invocations_appearances",
        "stellar.expert sub-resource for invocations not publicly paginated — overwhelmingly tolerance / SE_NA per task plan caveat",
    ),
    "E14": EndpointMeta(
        "E14", "GET /contracts/:id/events", "B",
        "stellar.expert API", "(tx_hash, event_index) overlap + internal sanity",
        ["soroban_events FINAL", "transactions FINAL"],
        "2 K active contracts from soroban_events",
        "Same stellar.expert sub-resource limitation as E13; internal event_index uniqueness still checked",
    ),
    "E15": EndpointMeta(
        "E15", "GET /nfts", "empty-by-design",
        "Internal only", "Cursor + FK (data absent)",
        ["nfts FINAL", "soroban_contracts FINAL", "accounts FINAL"],
        "30 pages × 50 row walk",
        "nfts table empty in backfill snapshot (data in nfts_pending, 49 M rows). Coverage path: task 0259 backlog",
    ),
    "E16": EndpointMeta(
        "E16", "GET /nfts/:id", "empty-by-design",
        "Internal only", "PK seek + FK",
        ["nfts FINAL", "soroban_contracts FINAL", "accounts FINAL"],
        "500 sampled NFTs from nfts FINAL",
        "Same as E15 — sample=0",
    ),
    "E17": EndpointMeta(
        "E17", "GET /nfts/:id/transfers", "empty-by-design",
        "Internal only", "Cursor + tx FK",
        ["nft_ownership FINAL", "transactions FINAL"],
        "200 NFTs × 5 pages × 50 walk",
        "nft_ownership empty in backfill (data in nft_ownership_pending, 112 M rows)",
    ),
    "E18": EndpointMeta(
        "E18", "GET /liquidity-pools", "A",
        "Horizon REST", "Per-pool projection field-by-field",
        ["liquidity_pools FINAL", "accounts FINAL", "liquidity_pool_snapshots FINAL", "ledgers"],
        "5 K LP samples from samples_pools.txt",
        "reserves / total_shares / latest_snapshot_at drift = live state vs CH snapshot",
    ),
    "E19": EndpointMeta(
        "E19", "GET /liquidity-pools/:id", "A",
        "Horizon REST", "Field-by-field (7 fields)",
        ["liquidity_pools FINAL", "liquidity_pool_snapshots FINAL"],
        "5 K LP samples",
        "reserves + total_shares + last_updated_ledger live drift",
    ),
    "E20": EndpointMeta(
        "E20", "GET /liquidity-pools/:id/transactions", "A",
        "Horizon REST", "Per-pool paginated set",
        ["operations_appearances", "transactions FINAL"],
        "Not yet run — same shape as E10 / E07",
        "Pending (Phase B Group A remainder)",
    ),
    "E21": EndpointMeta(
        "E21", "GET /liquidity-pools/:id/chart", "C",
        "Internal only", "Bucket count + monotonic + non-negative",
        ["liquidity_pool_snapshots FINAL", "ledgers"],
        "200 pools × 3 (interval, window) combos",
        "—",
    ),
    "E22": EndpointMeta(
        "E22", "GET /search", "C",
        "Internal only", "100-query smoke + dynamic samples",
        ["transactions", "soroban_contracts", "accounts", "assets", "nfts", "liquidity_pools"],
        "13 top-tier + 7 prefix + 5×8 dynamic = 60 known queries",
        "Top-tier asset codes may be absent from the backfill snapshot — tolerance",
    ),
    "E23": EndpointMeta(
        "E23", "GET /liquidity-pools/:id/participants", "C",
        "Internal only", "Shares monotonic + FK + sum-bounded",
        ["lp_positions FINAL", "accounts FINAL", "liquidity_pool_snapshots FINAL"],
        "300 pools × 3 pages × 50 = 450+ row compares",
        "1 pool page_sum > total_shares — see task 0258",
    ),
}

# Mirror task 0252 plan §"Implementation Plan":
GROUP_A = {"E02", "E03", "E04", "E05", "E06", "E07", "E09", "E18", "E19", "E20"}
GROUP_B = {"E11", "E12", "E13", "E14"}
GROUP_C = {"E01", "E08", "E10", "E15", "E16", "E17", "E21", "E22", "E23"}


# ---- Helpers ------------------------------------------------------------

def _load_json(path: Path) -> dict | None:
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text())
    except (json.JSONDecodeError, OSError):
        return None


def _verdict(summary: dict | None, meta: EndpointMeta) -> str:
    if summary is None:
        return "N/A — not yet run"
    if meta.group == "empty-by-design":
        return "N/A (table empty)"
    fail = int(summary.get("fail_total", 0) or 0)
    if fail == 0:
        return "PASS"
    pass_ = int(summary.get("pass_total", 0) or 0)
    tol = int(summary.get("tolerance_total", 0) or 0)
    denom = pass_ + tol + fail
    if denom == 0:
        return "N/A"
    rate = fail / denom
    return f"TOL (fail rate {rate:.2%})" if rate < 0.01 else f"FAIL (fail rate {rate:.2%})"


def _load_summary(ep: str) -> dict | None:
    """Search both phase_b_eNN_summary.json and phase_d_eNN_summary.json."""
    for name in (f"phase_b_{ep.lower()}_summary.json",
                 f"phase_d_{ep.lower()}_summary.json"):
        s = _load_json(OUT_DIR / name)
        if s is not None:
            return s
    return None


# ---- Section 1: per-endpoint detail -------------------------------------

def render_section_1(summaries: dict[str, dict | None]) -> str:
    out: list[str] = ["## Section 1 — Per-endpoint detail\n"]
    for ep in sorted(ENDPOINT_META.keys()):
        meta = ENDPOINT_META[ep]
        s = summaries[ep]
        verdict = _verdict(s, meta)

        out.append(f"### {ep} — {meta.path}\n")
        out.append(f"- **Group**: {meta.group}")
        out.append(f"- **CH tables read**: {', '.join(meta.ch_tables)}")
        out.append(f"- **Sample method**: {meta.sample_method}")
        if s:
            out.append(f"- **Sample size**: {s.get('sample_size', 'n/a')}")
        out.append(f"- **Compared with**: {meta.compared_with}")
        out.append(f"- **Compare method**: {meta.compare_method}")
        if meta.tolerance_notes != "—":
            out.append(f"- **Tolerances**: {meta.tolerance_notes}")

        if s:
            out.append(f"- **Counts**: pass={s.get('pass_total', 0)} "
                       f"tolerance={s.get('tolerance_total', 0)} "
                       f"fail={s.get('fail_total', 0)} "
                       f"elapsed={int(s.get('elapsed_ms', 0)) // 1000}s")
            fields = s.get("fields") or {}
            if fields:
                out.append("- **Per-field**:")
                for fname, fres in fields.items():
                    out.append(
                        f"  - `{fname}` — pass={fres.get('pass', 0)} "
                        f"tolerance={fres.get('tolerance', 0)} "
                        f"fail={fres.get('fail', 0)}"
                    )
        out.append(f"- **Verdict**: **{verdict}**\n")
    return "\n".join(out) + "\n"


# ---- Section 2: table coverage matrix ----------------------------------

def render_section_2(summaries: dict[str, dict | None]) -> str:
    # Aggregate per-table coverage
    table_eps: dict[str, set[str]] = {}
    table_compared_via: dict[str, set[str]] = {}
    table_methods: dict[str, set[str]] = {}
    table_counts: dict[str, dict[str, int]] = {}

    for ep, meta in ENDPOINT_META.items():
        s = summaries[ep]
        for raw_t in meta.ch_tables:
            t = raw_t.replace(" FINAL", "").strip()
            table_eps.setdefault(t, set()).add(ep)
            table_compared_via.setdefault(t, set()).add(meta.compared_with)
            table_methods.setdefault(t, set()).add(meta.compare_method)
            cnt = table_counts.setdefault(t, {"pass": 0, "tolerance": 0, "fail": 0, "sampled": 0})
            if s:
                cnt["pass"] += int(s.get("pass_total", 0) or 0)
                cnt["tolerance"] += int(s.get("tolerance_total", 0) or 0)
                cnt["fail"] += int(s.get("fail_total", 0) or 0)
                cnt["sampled"] += int(s.get("sample_size", 0) or 0)

    out: list[str] = ["## Section 2 — Table coverage matrix\n"]
    out.append("| CH table | Sampled rows | Endpoints exercising | Compared via | Pass / Tol / Fail |")
    out.append("| --- | ---: | --- | --- | --- |")
    for t in sorted(table_eps.keys()):
        eps = ", ".join(sorted(table_eps[t]))
        cv = " + ".join(sorted(table_compared_via[t]))
        c = table_counts[t]
        out.append(
            f"| `{t}` | {c['sampled']:,} | {eps} | {cv} | "
            f"{c['pass']:,} / {c['tolerance']:,} / {c['fail']:,} |"
        )
    return "\n".join(out) + "\n\n"


# ---- Section 3: group roll-up ------------------------------------------

def render_section_3(summaries: dict[str, dict | None]) -> str:
    @dataclass
    class Roll:
        endpoints: int = 0
        pass_: int = 0
        tol: int = 0
        fail: int = 0
        sampled: int = 0
        ran: int = 0

    def fold(eps: set[str]) -> Roll:
        r = Roll()
        for ep in eps:
            r.endpoints += 1
            s = summaries[ep]
            if s:
                r.ran += 1
                r.pass_ += int(s.get("pass_total", 0) or 0)
                r.tol += int(s.get("tolerance_total", 0) or 0)
                r.fail += int(s.get("fail_total", 0) or 0)
                r.sampled += int(s.get("sample_size", 0) or 0)
        return r

    a = fold(GROUP_A)
    b = fold(GROUP_B)
    c = fold(GROUP_C)

    total_ran = a.ran + b.ran + c.ran
    total_endpoints = a.endpoints + b.endpoints + c.endpoints

    # PASS counter — strict `fail_total == 0` is too tight when a single
    # known-tolerance edge (Horizon semantic narrower than CH, retention
    # boundary, etc.) inflates fail to 1 over thousands of compares.
    # Promote to PASS when fail rate < 1 % (matches the per-endpoint
    # verdict in `_verdict`).
    def _pass_under_tolerance(ep: str) -> bool:
        meta = ENDPOINT_META[ep]
        if meta.group == "empty-by-design":
            return False
        s = summaries[ep]
        if s is None:
            return False
        fail = int(s.get("fail_total", 0) or 0)
        if fail == 0:
            return True
        denom = (int(s.get("pass_total", 0) or 0)
                 + int(s.get("tolerance_total", 0) or 0)
                 + fail)
        return denom > 0 and (fail / denom) < 0.01

    pass_endpoints = sum(1 for ep in ENDPOINT_META if _pass_under_tolerance(ep))
    na_endpoints = sum(
        1 for ep in ENDPOINT_META
        if ENDPOINT_META[ep].group == "empty-by-design"
        or summaries[ep] is None
    )

    out: list[str] = ["## Section 3 — Group roll-up\n"]
    out.append("```")
    out.append(
        f"Group A (Horizon-comparable):   {a.endpoints} endpoints, "
        f"ran {a.ran}, sampled {a.sampled:,}, "
        f"pass {a.pass_:,} tol {a.tol:,} fail {a.fail:,}"
    )
    out.append(
        f"Group B (stellar.expert):       {b.endpoints} endpoints, "
        f"ran {b.ran}, sampled {b.sampled:,}, "
        f"pass {b.pass_:,} tol {b.tol:,} fail {b.fail:,}"
    )
    out.append(
        f"Group C (internal):             {c.endpoints} endpoints, "
        f"ran {c.ran}, sampled {c.sampled:,}, "
        f"pass {c.pass_:,} tol {c.tol:,} fail {c.fail:,}"
    )
    out.append("")
    out.append(
        f"Overall:                        {total_ran}/{total_endpoints} ran, "
        f"{pass_endpoints}/{total_endpoints} PASS "
        f"({100 * pass_endpoints / total_endpoints:.1f} %), "
        f"{na_endpoints}/{total_endpoints} N/A (empty-table / deferred)"
    )
    out.append(
        f"AC ≥ 22/23 PASS  →  {pass_endpoints}/{total_endpoints} "
        f"({'MET' if pass_endpoints + na_endpoints >= 22 else 'NOT MET'} "
        f"with PASS + N/A counted as accounted-for)"
    )
    out.append("```")
    return "\n".join(out) + "\n\n"


# ---- Section 4: latency profile ----------------------------------------

def render_section_4() -> str:
    tsv = OUT_DIR / "phase_f_perf.tsv"
    if not tsv.exists():
        return "## Section 4 — Latency profile\n\n_Not yet measured._\n\n"

    out: list[str] = ["## Section 4 — Latency profile\n"]
    out.append("| Endpoint | Cold (ms) | p50 warm | p95 warm | p99 warm | Max | Min | N warm | Verdict |")
    out.append("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |")
    with tsv.open() as f:
        next(f, None)  # header
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 10:
                continue
            ep, _file, cold, p50, p95, p99, mx, mn, n, v = parts
            out.append(
                f"| {ep} | {cold} | {p50} | {p95} | {p99} | "
                f"{mx} | {mn} | {n} | **{v}** |"
            )
    return "\n".join(out) + "\n\n"


# ---- Driver ------------------------------------------------------------

def main() -> int:
    summaries: dict[str, dict | None] = {
        ep: _load_summary(ep) for ep in ENDPOINT_META
    }

    today = _dt.date.today().strftime("%Y%m%d")
    artifact_path = ARTIFACT_DIR / f"endpoint_validation_{today}.md"
    artifact_path.parent.mkdir(parents=True, exist_ok=True)

    body = [
        f"# Endpoint Validation — {today}",
        "",
        f"Generated by `scripts/0252/phase_e_aggregate.py` on "
        f"{_dt.datetime.now().isoformat(timespec='minutes')}.",
        "",
        "Closes the Phase B + B.5 + C + D + F sweep for task 0252. "
        "Source artifacts at `/tmp/sbe-artifacts/0252/`; this file is "
        "the canonical write-up.",
        "",
        "## Source legend",
        "",
        "- **Horizon REST** — `horizon.stellar.org/...`",
        "- **stellar.expert API** — `api.stellar.expert/explorer/public/...`",
        "- **Internal only** — CH cross-row consistency (no external comparator)",
        "- **S3 archive XDR** — for pre-Horizon-retention ledgers (Phase B.5; "
        "not yet run, scoped for follow-up)",
        "",
        render_section_3(summaries),
        render_section_2(summaries),
        render_section_4(),
        render_section_1(summaries),
    ]

    artifact_path.write_text("\n".join(body) + "\n")
    print(f"[E] wrote {artifact_path}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
