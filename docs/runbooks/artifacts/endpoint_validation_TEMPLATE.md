# ClickHouse endpoint validation report — TEMPLATE

> Filled by task 0252 Phase E aggregator. Replace placeholders with
> aggregated values from `/tmp/sbe-artifacts/endpoint_validation_*.tsv`
> emitted by Phase A/B/C/D scripts. See task 0252 §Reporting Shape for
> the contract.

**Task:** [0252 — CH endpoint parity vs Horizon / stellar.expert](../../../lore/1-tasks/active/0252_VALIDATION_clickhouse-endpoint-parity-against-stellar-apis.md)
**Operator:** {{OPERATOR}}
**Date:** {{YYYY-MM-DD}}
**CH target:** `ch-prod-01`, container `app-clickhouse-1`, range `50,457,424` → `62,527,999`
**Backfill state:** post-0228 Phase 5 (repair-tier1, asset-aggregates, nft-reclassify all complete)

## Verdict: {{GREEN | YELLOW | RED}}

{{One-paragraph headline. State pass rate, blocking diffs (if any),
and the go-live signal.}}

---

## Section 1 — Per-endpoint detail

> One stanza per endpoint (23 total). Fill from Phase A/B/C/D TSV outputs.

### E01 — GET /network/stats

- **CH tables read:** `accounts`, `assets`, `ledgers`, `transactions`, `soroban_contracts`
- **Sample method:** single-snapshot aggregate query
- **Sample size:** 1
- **Compared with:** Internal only (cross-row sanity)
- **Compare method:** count() consistency check
- **Tolerances:** ±0 (single-snapshot)
- **Coverage:** N/A (aggregate)
- **Per-field accuracy:** {{TBD}}
- **Verdict:** {{TBD}}

### E02 — GET /transactions

- **CH tables read:** `transactions`, `transaction_hash_index`, `transaction_participants`, `ledgers`
- **Sample method:** {{}}
- **Sample size:** {{}}
- **Compared with:** Horizon REST `/transactions?order=desc`
- **Compare method:** hash-set per ledger window (paginated cursor walk)
- **Tolerances:** Horizon retention boundary at L≥56657428
- **Coverage:** {{N}} / 3,540,956,296 rows
- **Per-field accuracy:** {{TBD}}
- **Verdict:** {{TBD}}

### E03 — GET /transactions/:hash

- **CH tables read:** `transaction_hash_index` (via dict), `transactions`, `operations_appearances`, `transaction_participants`, `soroban_events`, `soroban_invocations_appearances`
- **Sample method:** random by ledger DESC
- **Sample size:** 50
- **Compared with:** Horizon REST `/transactions/{hash}`
- **Compare method:** field-by-field (7 fields)
- **Tolerances:** op_count: Horizon successful-only vs CH all-ops semantic
- **Coverage:** 50 / 3,540,956,296 rows
- **Per-field accuracy:** {{TBD}}
- **Verdict:** {{TBD}}

### E04 — GET /ledgers

{{stanza}}

### E05 — GET /ledgers/:sequence

{{stanza}}

### E06 — GET /accounts/:account_id

{{stanza}}

### E07 — GET /accounts/:account_id/transactions

{{stanza}}

### E08 — GET /assets

{{stanza}}

### E09 — GET /assets/:id

{{stanza}}

### E10 — GET /assets/:id/transactions

{{stanza}}

### E11 — GET /contracts/:contract_id

- **CH tables read:** `soroban_contracts`, `soroban_invocations_appearances`, `ledgers`
- **Sample method:** stratified by `contract_type` (Token / Nft / Fungible / Other / NULL)
- **Sample size:** 10 contracts
- **Compared with:** stellar.expert API `/explorer/public/contract/{id}`
- **Compare method:** field-by-field (deployer_id, deployed_at_ledger, contract_type)
- **Tolerances:** stellar.expert may classify contract_type differently — record divergence per-contract
- **Coverage:** 10 / 321,364 rows
- **Per-field accuracy:** {{TBD}}
- **Verdict:** {{TBD}}

### E12 — GET /contracts/:contract_id/interface

{{stanza}}

### E13 — GET /contracts/:contract_id/invocations

{{stanza}}

### E14 — GET /contracts/:contract_id/events

{{stanza}}

### E15 — GET /nfts

- **CH tables read:** `nfts`, `nfts_pending`
- **Sample method:** N/A
- **Sample size:** 0
- **Compared with:** Internal only
- **Compare method:** empty by design (`nfts` = 0 rows per 0228 Phase 5; `nfts_pending` parking lot)
- **Verdict:** N/A — no NFT-classified contracts in union

### E16 — GET /nfts/:id

{{stanza — likely N/A as E15}}

### E17 — GET /nfts/:id/transfers

{{stanza}}

### E18 — GET /liquidity-pools

{{stanza}}

### E19 — GET /liquidity-pools/:id

{{stanza}}

### E20 — GET /liquidity-pools/:id/transactions

{{stanza}}

### E21 — GET /liquidity-pools/:id/chart

{{stanza}}

### E22 — GET /search

{{stanza}}

### E23 — GET /liquidity-pools/:id/participants

{{stanza}}

---

## Section 2 — Table coverage matrix

| CH table                          |          Rows | Sampled rows |    Endpoints exercising | Compared via              | Compare method | Pass / Tol / Fail |
| --------------------------------- | ------------: | -----------: | ----------------------: | ------------------------- | -------------- | ----------------- |
| `account_balances_current`        |    47,190,041 |        {{N}} |                    {{}} | {{}}                      | {{}}           | {{}}              |
| `accounts`                        |    13,884,923 |        {{N}} |                E06, E07 | Horizon REST              | field          | {{}}              |
| `assets`                          |       300,610 |        {{N}} |           E08, E09, E10 | Horizon REST              | field + count  | {{}}              |
| `ledgers`                         |    12,070,576 |        {{N}} |                E04, E05 | Horizon + Tier 5          | continuity     | {{}}              |
| `liquidity_pool_snapshots`        |   250,392,182 |        {{N}} |                     E21 | Internal only             | continuity     | {{}}              |
| `liquidity_pools`                 |        50,126 |        {{N}} | E18, E19, E20, E21, E23 | Horizon REST              | field          | {{}}              |
| `lp_positions`                    |       103,904 |        {{N}} |                     E23 | Internal only             | sum/count      | {{}}              |
| `nft_ownership`                   |             0 |            0 |                     E17 | empty by design           | n/a            | n/a               |
| `nft_ownership_pending`           |   112,301,444 |        {{N}} |                     E17 | Internal only             | row sanity     | {{}}              |
| `nfts`                            |             0 |            0 |           E15, E16, E17 | empty by design           | n/a            | n/a               |
| `nfts_pending`                    |    48,854,535 |        {{N}} |           E15, E16, E17 | Internal only             | row sanity     | {{}}              |
| `operations_appearances`          | 5,832,066,715 |        {{N}} |                E03, E20 | Horizon (indirect via tx) | per-tx count   | {{}}              |
| `soroban_contracts`               |       321,364 |        {{N}} | E11, E12, E13, E14, E22 | stellar.expert API        | field + iface  | {{}}              |
| `soroban_events`                  | 8,676,825,779 |        {{N}} |                     E14 | stellar.expert API        | per-event diff | {{}}              |
| `soroban_invocations_appearances` |   718,961,248 |        {{N}} |                     E13 | stellar.expert API        | count          | {{}}              |
| `transaction_hash_index`          | 3,540,956,296 |        {{N}} |                     E03 | Tier 5 hash-set           | hash-set       | {{}}              |
| `transaction_participants`        | 8,191,652,507 |        {{N}} |                E03, E07 | Internal only             | per-tx count   | {{}}              |
| `transactions`                    | 3,540,956,296 |        {{N}} |      E02, E03, E07, E20 | Horizon REST + Tier 5     | hash + field   | {{}}              |
| `wasm_interface_metadata`         |         3,216 |        {{N}} |                     E12 | stellar.expert API        | field          | {{}}              |

Row counts as of 2026-05-21 (post-0228 Phase 5). Update with current
`SELECT name, total_rows FROM system.tables WHERE database = 'default'`
at validation time if drift > 0.

---

## Section 3 — Group roll-up

```
Group A (Horizon-comparable):    10 endpoints, {{M}} CH tables, {{K}} sample compares
  Pass: {{X}}  Tolerance: {{Y}}  Fail: {{Z}} ({{description, link to spawned task}})

Group B (stellar.expert-only):   4 endpoints, {{M}} CH tables, {{K}} compares
  Pass: {{X}}  Tolerance: {{Y}}  Fail: {{Z}}

Group C (internal consistency):  9 endpoints, {{M}} CH tables, {{K}} compares
  Pass: {{X}}  Tolerance: {{Y}}  Fail: {{Z}}

Overall: {{K}} compares; {{P}}/23 endpoints PASS ({{P/23 * 100}} %)
AC threshold: 22/23 (95 %)
AC verdict: {{PASS | FAIL}}
```

---

## Source legend

| Source               | URL pattern                                                                                  |
| -------------------- | -------------------------------------------------------------------------------------------- |
| `Horizon REST`       | `https://horizon.stellar.org/...`                                                            |
| `stellar.expert API` | `https://api.stellar.expert/explorer/public/...`                                             |
| `Tier 5 hash-set`    | Internal cross-reference, see [phase6_validation_20260521.md](phase6_validation_20260521.md) |
| `Internal only`      | CH cross-row consistency (SQL invariants); no external API                                   |
| `S3 archive XDR`     | `s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/...`                                 |

---

## Spawned follow-ups

> Every unexpected divergence here spawns a backlog bug task with
> `related_tasks: ['0252']`. Update with task IDs once spawned.

- [ ] {{Endpoint EXX divergence description}} — task {{NNNN}}
- [ ] ...

---

## Sign-off

- **Operator:** {{name}}
- **Date:** {{YYYY-MM-DD}}
- **0252 status:** {{active → archive | continuing to Phase F}}
- **Linked artifacts:** [phase6_validation_20260521.md](phase6_validation_20260521.md), [task 0207](../../../lore/1-tasks/archive/0207_FEATURE_clickhouse-endpoint-queries-reference-set.md)
