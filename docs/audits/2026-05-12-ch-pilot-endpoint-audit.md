# CH Pilot Endpoint Audit — 23/23 via `/compare-with-stellar-api`

**Date:** 2026-05-12
**Source:** `crates/db-clickhouse/schema/init.sql` (post-PR-#175 hybrid-surrogate)
**Data:** fresh 64k-ledger backfill (62016000-62079999), 11.6 GB raw, real mainnet, ingested via `backfill-runner --target clickhouse` with PR-#175 non-stub writer.
**Method:** `/compare-with-stellar-api` skill against Horizon API + stellar.expert API + raw XDR (py-stellar-sdk 14.0.0).

## Verdict scorecard

| Category                                                     | Count | Endpoints                                                                 |
| ------------------------------------------------------------ | ----- | ------------------------------------------------------------------------- |
| ✅ Full PASS (XDR ground truth or 3-source MATCH)            | 15    | E01, E02, E03, E04, E05, E07, E10, E11, E12, E13, E14, E18, E19, E22, E23 |
| ✅ PASS-by-context (empty result correct for backfill range) | 1     | E20                                                                       |
| ⚠️ PASS w/ state-NULL gap                                    | 3     | E08, E09, E21                                                             |
| 🚨 CRITICAL GAP (no LedgerEntry full ingest)                 | 1     | E06                                                                       |
| 🚨 DATA UNUSABLE (0118 false positives)                      | 3     | E15, E16, E17                                                             |

## Findings

### ✅ Full PASS endpoints

See per-row XDR verdicts. Fee-bump inner-source semantics correct, app_order=1-based via paging_token, SAC derivation byte-for-byte MATCH, CAP-67 fee events in `TransactionMetaV4.events[]` confirmed, `parameters[0]` = target contract verified, caller-tree correct for both auth-required and contract-caller subcalls.

**§5.1 win materialized:** E14 contract events return full inline decoded payload (`topics_xdr`, `data_xdr` actually JSON not raw XDR per PR #175 writer design); CH more complete than stellar.expert (which filters out CAP-67 fee events).

### State-NULL gaps — same architectural root cause

**E08 / E09 / E21** all share the gap: parser-emitted `Extracted*` structs don't carry computed aggregates; CH writer stages as-is; **CH writer has no equivalent of PG's `recompute_asset_aggregates` (task 0194)** post-write step.

| Field                                  | Owner per ADR 0043                      | CH coverage                          |
| -------------------------------------- | --------------------------------------- | ------------------------------------ |
| `assets.holder_count`                  | indexer (on-chain) — PG via task 0194   | ❌ not ported to CH writer           |
| `assets.total_supply`                  | indexer (on-chain) — PG via task 0194   | ❌ not ported to CH writer           |
| `liquidity_pool_snapshots.tvl`         | Lambda 2 (off-chain prices) — task 0199 | ❌ blocked-on-oracle (even PG empty) |
| `liquidity_pool_snapshots.volume`      | Lambda 2 — task 0199                    | ❌ blocked-on-oracle                 |
| `liquidity_pool_snapshots.fee_revenue` | Lambda 2 — task 0199                    | ❌ blocked-on-oracle                 |

**Decision:** Port task 0194 recompute to CH writer **deferred until CH pilot + backfill done**. Task 0199 (LP analytics) **blocked-on-oracle** (waiting on Oskar's price API).

**Frontend impact while blocked:**

- §6.13 LP list — `tvl` column shows `—` / null
- §6.14 LP detail — `tvl`, `volume`, `fee_revenue` widgets show `—`
- §6.14 LP chart — entire chart bucketing works but all 3 series NULL (display empty chart with "data not available" message recommended)

### E06 — CRITICAL GAP: account state ingestion

**Problem:** CH `accounts` table populates from `transaction_participants` driver — skeleton rows only. Real account state (`sequence_number`, `home_domain`) + trustlines (`account_balances_current`) require `LedgerEntryAccount` / `LedgerEntryTrustLine` ingestion. CH writer has staging path for these fields BUT parser `extract_account_states()` emits conditionally on observed LedgerEntry changes. In a 64k-ledger backfill window most accounts never have their LedgerEntry updated → skeleton rows persist.

**Example:** `GARDNV3Q7YGT4AKSDF25LT32YSCCW4EV22Y2TV3I2PU2MMXJTEDL5T55` — CH stores `seqnum=0, home_domain=null, 0 balances` vs Horizon shows live account with `seqnum=148e15, home_domain="ultracapital.xyz", 12 861 XLM balance`.

**Resolution:** in CH pilot scope — needs **initial-snapshot mechanism** on backfill start (read live state via Soroban RPC `getLedgerEntries` for all observed accounts at the backfill window's start ledger). Task spawned: **0214**.

### E15/E16/E17 — DATA UNUSABLE (0118 false positives)

**Top 5 "NFT" contracts in CH = all fungibles:**

| Contract             | Rows    | Reality                  |
| -------------------- | ------- | ------------------------ |
| `CAS3J7GY` (XLM SAC) | 421 871 | XLM native asset wrapper |
| `CCW67TSZ`           | 263 023 | fungible token           |
| `CB23WRDQ`           | 202 290 | fungible                 |
| `CBROEYKB`           | 118 641 | fungible                 |
| `CAUIKL3I`           | 117 601 | fungible                 |

Zero `nfts` rows with `name IS NOT NULL`. `token_id` values are amount stroops (e.g. `"80367"` = 0.0080367 XLM).

**Root cause:** Task 0118 Phase 2 classifier returns `Other` when WASM not observed in backfill window. Permissive emit policy → false positives. Filter only blocks contracts with `contract_type=Fungible` (classifier verdict) — but those are minority.

**Fix:** Task 0118 reactivated. Phase 3 cleanup SQL + ingester filter strengthen for pre-window WASM-less contracts.

**Frontend impact:** §6.11 NFT list, §6.12 NFT detail/transfers — endpoints unreliable until cleanup + filter strengthen lands.

## 6 PR #166 anti-patterns confirmed

| #   | Anti-pattern                           | Where manifested                                                                                                                                                                                                                                                                                                                                   |
| --- | -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | CAP-67 fee event semantics             | E14 — events in `TransactionMetaV4.events[]` w stages BEFORE/AFTER_ALL_TXS                                                                                                                                                                                                                                                                         |
| 2   | WebFetch hallucinations                | E05 — fabricated source/fee values caught; switched to curl + python3 json.load                                                                                                                                                                                                                                                                    |
| 3   | StrKey CRC                             | E08 — 100% valid; SAC derivation byte-for-byte cross-check bonus                                                                                                                                                                                                                                                                                   |
| 4   | `parameters[0]` positional             | E10 — XDR `invoke_contract.contract_address` = target contract; `[2]` = first call arg                                                                                                                                                                                                                                                             |
| 5   | stellar.expert /contract field surface | E11 — 9 fields, no ledger_seq, `invocations=null` ≠ 0                                                                                                                                                                                                                                                                                              |
| 6   | Caller-tree auth vs auth-less          | E13 — diagnostic events present in protocol-25 archive XDR for our backfill range (read via S3 / Galexie-style archive stream where diagnostics are emitted; not a guarantee about public Horizon nodes, where diagnostics depend on core config). PR #166 #6 note "no diagnostics post-protocol-22" is at least incomplete for archive consumers. |

## Method insights (for skill notes)

1. **stellar.expert API** (`https://api.stellar.expert/explorer/public/<entity>/<id>`) over HTML page — avoids WebFetch JS-shell issue.
2. **curl + python3 json.load** mandatory — WebFetch fabricates primitives.
3. **Two shapes from `/tx/<hash>`** on stellar.expert — Horizon-mirror (full primitives) vs Minimal (base64 XDR only). Fallback to Horizon for missing fields.
4. **Horizon 2.30+ omits `result_meta_xdr`** for Soroban-era tx. Workaround: Soroban RPC `getLedgers` → `LedgerCloseMeta` v2 → `tx_processing[i].tx_apply_processing.v4.events[]`.
5. **SAC derivation cross-check** = strongest ground truth: `stellar_sdk.Asset(code, issuer).contract_id(PUBLIC)`.
6. **paging_token decode** = `app_order = (toid >> 12) & 0xFFFFF` (1-based per task 0172).
7. **Fold-count amount fields** per ADR 0033/0034: `soroban_invocations_appearances.amount` + `operations_appearances.amount` are NOT token values. Init.sql docstrings added 2026-05-12 (same PR as this audit).

## Cross-references

- ADR 0044 — CH pilot parallel store
- ADR 0043 — field allocation rule (indexer vs Lambda 2)
- ADR 0033 — soroban_events_appearances PG-side fold convention
- ADR 0034 — soroban_invocations_appearances PG-side fold convention
- Task 0118 — NFT false positives (Phase 2 done; Phase 3 reactivated 2026-05-12)
- Task 0194 — `assets.holder_count` / `total_supply` recompute (PG-only; CH port deferred)
- Task 0199 — LP analytics (blocked-on-oracle)
- Task 0207 — CH endpoint queries reference set (this audit's input)
- Task 0214 — CH initial-snapshot mechanism for account state (spawned 2026-05-12)
- Task 0215 — LP analytics blocked-endpoint FE-impact doc (spawned 2026-05-12, blocked-by 0199)
- PR #166 — `/compare-with-stellar-api` skill hardening (6 anti-patterns)
- PR #175 — CH writer for ADR 0044 pilot schema
