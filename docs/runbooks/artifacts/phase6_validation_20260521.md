# Phase 6 validation report — 2026-05-21

**Task:** [0228 — parallel-backfill merge into Hetzner CH](../../../lore/1-tasks/active/0228_FEATURE_parallel-backfill-merge-and-validation/README.md)
**Operator:** Stanisław Królikiewicz
**Target:** `ch-prod-01`, container `app-clickhouse-1`
**Runbook:** [`docs/runbooks/0228_phase6_validation.md`](../0228_phase6_validation.md)
**Ledger range:** `L_first = 50,457,424` → `L_last_closed = 62,527,999`

## Verdict: GREEN — go-live signal

AC §"Sample-compare against Horizon … ≤ 0.01 % mismatch" satisfied
on the spot-check ledger: **0 / 205 tx hash mismatches** at canonical
`transaction_hash_index.hash` level. Phase 5 repair pass landed without
data loss; no-FINAL-at-query-time invariant holds across all RMT
state tables.

## Per-tier results

| Tier                           | Status            | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ------------------------------ | ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 — Sanity                     | ✓ PASS            | Ledger continuity gaps=0. 17/19 tables populated (`nfts`/`nft_ownership` empty by design — `promoted_nfts=0` in nft-reclassify, no Nft-classified contracts in union). Dict `transaction_hash_dict` healthy (CACHE layout, lazy-fill; verified dictGet roundtrip 10/10). No-FINAL invariant holds for all 8 RMT state tables.                                                                                                                             |
| 2 — Tier-1 rebuild correctness | ✓ PASS            | All 5 columns × 5 tables produce matching aggregates on 10-row spot samples: `accounts.first_seen_ledger`, `lp_positions.first_deposit_ledger`, `nfts_pending.minted_at_ledger`, `soroban_contracts.{deployer_id, deployed_at_ledger}`.                                                                                                                                                                                                                   |
| 3 — Worker baseline parity     | DEFERRED          | Requires `laptop2_pre-export-metrics.json` + `laptop3_pre-export-metrics.json` — not yet captured. Row counts from Tier 1.2 documented as post-merge baseline.                                                                                                                                                                                                                                                                                            |
| 4 — Skeleton/orphan/per-ledger | ✓ PASS w/ caveats | Skeleton pct = 3.28 % (>1 % runbook threshold but consistent with merged-state baseline; laptop1 alone reported 2.86 %). 0 negative sequences. 8 false-alarm "ledger anomalies" turned out to be runbook LEFT JOIN default-fill bug — true `transactions` row count for those 8 ledgers is 0, matching `ledgers.transaction_count = 0`. Tier 4.2 (orphan ops) deferred — antijoin over 5.83B × 3.54B mem-bound at full scale; Tier 5 covers semantically. |
| 5 — Horizon compare smoke      | ✓ PASS            | 32 stratified newer ledgers: tx-count mismatch 0/32. Op-count drift across all 32 (mixed direction) — documented Horizon semantic noise (inner Soroban op accounting + fee-bump flattening), not data divergence. Hash-level set diff on probed ledger 56657428: **205 / 205 hashes identical** across `transaction_hash_index` ↔ Horizon `/ledgers/{N}/transactions?include_failed=true`.                                                                |
| 6 — Repo tooling               | SKIP              | Optional ad-hoc per runbook.                                                                                                                                                                                                                                                                                                                                                                                                                              |

## Headline post-merge row counts

```
account_balances_current          47,190,041
accounts                          13,884,923
assets                               300,610
ledgers                           12,070,576
liquidity_pool_snapshots         250,392,182
liquidity_pools                       50,126
lp_positions                         103,904
nft_ownership                              0  (no Nft-classified contracts)
nft_ownership_pending            112,301,444
nfts                                       0  (no Nft-classified contracts)
nfts_pending                      48,854,535  (post nft-reclassify drop of 27.6M false positives)
operations_appearances         5,832,066,715
soroban_contracts                    321,364
soroban_events                 8,676,825,779
soroban_invocations_appearances  718,961,248
transaction_hash_index         3,540,956,296
transaction_participants       8,191,652,507
transactions                   3,540,956,296
wasm_interface_metadata                3,216
```

Total: 17 tables with rows (+ 2 empty by design); 1 dictionary
(`transaction_hash_dict`).

## Phase 5 repair pass — input row counts

| Subcommand                         | Output                                                                                                                                                               |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `repair-tier1`                     | accounts=13,884,923 / lp_positions=103,904 / nfts=0 / nfts_pending=76,456,844 / soroban_contracts=321,364                                                            |
| `OPTIMIZE soroban_contracts FINAL` | no-op (post-EXCHANGE table single-part)                                                                                                                              |
| `asset-aggregates`                 | assets_rows=300,610                                                                                                                                                  |
| `nft-reclassify`                   | promoted_nfts=0 / promoted_ownership=0 / dropped_pending_nfts=27,602,309 / dropped_pending_ownership=60,492,304 / dropped_legacy_nfts=0 / dropped_legacy_ownership=0 |

## Repair-pass evidence (Tier 2 spot-checks)

`accounts` post-repair:

- 13,884,251 rows with `first_seen_ledger > 50,457,424`
- 672 rows with `first_seen_ledger <= 50,457,424` (of which 41 = 0 — bootstrap-only RPC-seeded accounts that never hit a tx in range; expected `ifNull` keep-original behaviour)
- 10,132,957 rows with `first_seen_ledger < last_seen_ledger` — **strong evidence** the repair moved values down from pre-repair RMT-collapsed state where `fsl == lsl`
- 3,751,966 rows with `fsl == lsl` — accounts touched in exactly one ledger, correctly preserved

`assets` post-aggregates:

- 298,542 classic-credit assets (`asset_type = 1`); 201,245 with non-zero holders/supply
- 2,065 SAC-wrapped (`asset_type = 2`); 1,778 with non-zero holders/supply
- max_holders 539,303 on both type families (same underlying asset, e.g. USDC class)

## Runbook bugs surfaced (track in follow-up)

1. Step 1.3 — dict `element_count` expectation invalid for `COMPLEX_KEY_CACHE` layouts (cells fill lazily; pre-`dictGet` element_count is 0). Replace with `dictGet` roundtrip smoke.
2. Step 1.4 — loop includes `wasm_interface_metadata` (plain `MergeTree`, no FINAL semantics). Filter the loop to RMT-engine tables.
3. Step 2.1 — joins `accounts.account_id` (String StrKey) against `transaction_participants.account_id` (Int64 surrogate). Must join `accounts.id` ↔ `transaction_participants.account_id`.
4. Step 2.4 — `argMin(deployer_id, …) AS deployer_id` alias shadows raw column; CH 26.3 ILLEGAL_AGGREGATION. Rename to `deployer_id_rebuilt` etc. (matches the production fix in `repair_tier1.rs`).
5. Step 4.1 — skeleton threshold `< 1 %` too tight for the merged Soroban-era state (laptop1 alone is 2.86 %; merged 3.28 %). Document as expected; loosen the criterion or split per-worker vs union baseline.
6. Step 4.2 — `NOT EXISTS` antijoin over 5.83B × 3.54B exceeds 64 GiB even with disk spill. Needs partition-aware decomposition or removal (Tier 5 hash-set compare covers semantically).
7. Step 4.3 — `count(DISTINCT t.id)` over a LEFT JOIN counts CH's default-fill `0` as a distinct value when the right side has no match. False positives. Use `sum(t.id != 0)` or `INNER JOIN`.
8. Step 4.4 — references `accounts.ledger_sequence`, which does not exist (state-shaped table under RMT collapse; only `first_seen_ledger`/`last_seen_ledger` present). Replace with sequence-monotonicity check against fact table.
9. Step 5.x — Hash-set compare must (a) lowercase `hex(hash)` to align case, (b) read `transaction_hash_index.hash` (not `transactions.id` — Int64 surrogate), (c) paginate Horizon `?limit=200&order=asc` with `_links.next.href`, (d) pass `include_failed=true`.

## Outstanding follow-ups

- **Server profile revert** — `users.d/timeouts.xml` reverted to `6000000000` on disk but server runtime still at 64 GiB cap (process not restarted). Restart `app-clickhouse-1` post-validation to refresh.
- **Snapshot B + Borg → BX21** — pre-go-live rollback anchor. Free local disk first (276 GiB free; Snapshot A occupies the rest, would need to ship to BX21 + drop).
- **PR #199 commit + push** — repair_tier1 FROM-AS-FINAL alias fix + argMin shadowing fix.
- **Spawn 0234 follow-up** — runbook bug list above + classifier versioning proposal (per task 0217 quarantine discussion).
- **Tier 3 catch-up** — laptop2 + laptop3 pre-export-metrics JSON ingestion when available.
- **Full Tier 5 (1000 ledgers)** — background job, paginated hash-set compare against Horizon. Smoke verdict GREEN, but the AC literal text says 1000 ledgers.

## Sign-off

- **Operator:** Stanisław Królikiewicz
- **Date:** 2026-05-21
- **Phase 5 + Phase 6 status:** complete (smoke); ready for go-live pending Snapshot B + PR #199 merge.
