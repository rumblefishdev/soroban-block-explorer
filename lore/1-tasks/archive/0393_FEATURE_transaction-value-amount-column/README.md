---
id: '0393'
title: 'FEATURE: transaction value ("amount moved") column — net-settled per-asset value for tx-list views'
type: FEATURE
status: done
related_adr: []
related_tasks: ['0359', '0383', '0261', '0247']
tags:
  [
    clickhouse,
    indexer,
    xdr-parser,
    frontend,
    transactions,
    effort-large,
    priority-medium,
  ]
milestone: 1
links:
  - crates/db-clickhouse/schema/init.sql
  - crates/xdr-parser/src/ledger_value.rs
  - crates/xdr-parser/src/net_settled.rs
  - crates/db-clickhouse/src/persist/stage.rs
  - crates/xdr-parser/tests/net_settled_real_corpus.rs
history:
  - date: '2026-07-15'
    status: backlog
    who: karolkow
    note: >
      Task drafted from a research + design investigation. Requirement:
      surface a "value moved" figure per transaction in list views (fee is
      uninformative for locating a transaction). Definition, storage,
      source, and cost are researched below; implementation not started.
  - date: '2026-07-15'
    status: active
    who: karolkow
    note: >
      Promoted to active to begin implementation.
  - date: '2026-07-20'
    status: active
    who: karolkow
    note: >
      Fundamental redesign shipped + verified. Value now reads the authoritative
      LEDGER (Account / Trustline / ContractData balance deltas) for EVERY tx — the
      event-value path is deleted (events are spoofable logs). Asset vocabulary split
      into two per-domain enums: EventAsset {Native, Credit, Bespoke} (event decoder,
      presence-only) and LedgerAsset {Native, Credit, SacWrapped, Bespoke} (ledger
      value reader). Cross-validated 1:1 on real mainnet data vs Horizon /effects
      (incl. protocol-23 contract-effects) + stellar CLI — 7-fixture gated corpus +
      prod-resolver E2E test. Spawned tasks 0410-0418. All green; uncommitted.
  - date: '2026-07-21'
    status: done
    who: karolkow
    note: >
      Done — implementation complete, verified, committed + pushed. Committed in 4
      commits (refactor: ledger-not-events read + EventAsset/LedgerAsset split;
      feat: bespoke type-3 surface + bloom skip index + api-types regen; test:
      8-fixture real-mainnet corpus + prod-resolver E2E + decode_meta harness;
      docs: arch docs + README + spawns) — 40 files. Verification closed the tail:
      claimable-balance added as 8th fixture (revealed pass-through netting correct
      + documents the 0413 issuer-side gap); every case cross-validated 1:1 vs
      Horizon /effects + stellar CLI. All code acceptance criteria [x]. DEFERRED to
      deployment/ops (not code gaps): S3 re-ingest for history, and the read-path
      RELEASE GATE (owned by 0417 companion table). Naming nit — `classic_*_deltas`
      now covers Soroban ContractData too; rename to `ledger_*` folds into 0418.
      Spawned follow-ups 0412-0418.
---

# FEATURE: transaction value ("amount moved") column

## Summary

Surface a **value-moved figure per (transaction, asset)** in transaction-list
views (the global transactions list and the per-account transactions list), so
a transaction can be located and scanned by amount rather than by fee. On
Stellar the fee is near-constant (~0.00001 XLM) and useless for this. The
figure is a **single aggregated value per (transaction, asset)** — a product
constraint — defined as the **net settled value** (see
[S-formula-and-edge-cases](notes/S-formula-and-edge-cases.md)).

## Context

### There is no single "transaction amount" in the protocol

A Stellar transaction is a **container of N operations**. The only scalar
money field at transaction grain is `fee_charged` (and `max_fee`); the Horizon
`Transaction` object exposes no amount/value/total. Value lives on
**operations** (13 operation types carry a money field: payment, path payment,
create account, offers, claimable balance, clawback, LP deposit/withdraw).
This is a protocol-level fact, not an oversight — the reference explorer and
the major third-party explorers all omit a per-transaction amount in their
transaction lists (they show operation count, native value, or a per-transfer
list). See [R-metric-research](notes/R-metric-research.md).

Because the product requires a **single** aggregated value anyway, this task
defines the least-wrong one.

### Value moves via three mechanisms

1. **Native XLM** — classic native asset, 7 decimals. Amount in the operation
   body / result (path-payment claim atoms).
2. **Classic issued assets** (`credit_alphanum4/12`) — issuer + trustline, 7
   decimals. Same operation-body source.
3. **Soroban tokens** — moved inside `invoke_host_function`, surfaced as
   `transfer`/`mint`/`burn`/`clawback` **events**, decimals per token metadata.
   Two flavours: **SAC** (built-in contract wrapping a classic asset incl.
   XLM) and **bespoke SEP-41** contract tokens.

Protocol 23 / CAP-67 (mainnet activation ledger **58 762 517**, ~Sept 2025)
makes classic operations **also** emit the unified event format, so from that
ledger onward a single event stream covers all three mechanisms. Before it,
classic movements are only in the operation body (events cover Soroban only).

### What exists today

- Transfer **amounts are not stored** in a queryable column. The presence
  indexes (`operation_asset_appearances`, `transaction_participants`) carry
  fold-counts, not money. The transaction-detail page parses amounts from
  archive XDR on demand.
- Two extraction paths already compute the raw amounts but **discard** them:
  - The classic operation/result path (task 0359) extracts path-payment
    claim-atom amounts (both legs) and classic operation amounts; today they
    feed LP `gross_volume_a` and the detail JSON only.
  - The Soroban token-event decoder (task 0383) decodes
    transfer/mint/burn/clawback including the CAP-67 unified asset identity,
    but explicitly does not read the amount out of `data_xdr`.
- The Soroban token-flow **historical backfill (0383) has not been run** on
  prod yet — only the live ingest hook is deployed. This is an opportunity:
  the amount backfill can run in the same CH-local pass (see below).
- Token `decimals`/`symbol` live in `soroban_contract_metadata`, populated
  from a possibly-different ledger than the transfer — so scaling must happen
  **at read time** (raw `Int128` stored, decimals joined on read), the same
  pattern already used for balances.

## Decision: net settled value

**`net_settled` per (transaction, asset) = `max(Σ positive account deltas, Σ negative account deltas)`.**

Per transaction and asset: sign each transfer (`from` −amount, `to` +amount),
accumulate a signed delta per account, then take the larger of the total gained
and total lost. This is the industry "net settled value" — it nets out
routing/pass-through hops automatically (a pass-through account has delta ~0),
which gross summation would double-count.

Three rules complete the definition:

1. **`max` of both sides** (not just the gained side) — required so burns and
   payments-to-issuer (one-sided, nobody gains) are counted, not shown as 0.
2. **Native XLM canonicalised to one `asset_id`** before grouping — native has
   two surrogate conventions in the codebase; split deltas would not cancel.
3. **`fee` events excluded** — the transaction fee is tracked separately; its
   native outflow must not inflate the native figure.

Rationale, the alternatives rejected (gross, `Σ+` only, `Σ|delta|`), the
edge-case stress tests (mint/burn/issuer/mixed), and the representation-
robustness argument are in
[S-formula-and-edge-cases](notes/S-formula-and-edge-cases.md). Prod
measurements that size the work and bound the edge cases are in
[R-prod-measurements](notes/R-prod-measurements.md).

## Implementation Plan

### 1. Storage — reuse the existing per-(tx, asset) index

Add to `operation_asset_appearances` (already keyed `(asset_id, ledger, tx)`):

- `net_settled Nullable(Int128)` — raw, net-settled per (tx, asset). Scaled at read. NULL = not computed yet (backfill), 0 = genuinely nothing settled.

> **Implemented:** `transfer_count` was **dropped**. It was justified for the
> "+ N others" affordance, but that count is really "N other **assets**" per tx —
> a read-time count of the asset rows, not a stored per-asset folded-transfer
> count. Storing it on a 9.5 B-row table for no reader was not worth it.

Reusing this table avoids re-storing the incompressible `transaction_id`
column (~71 GiB) that a separate table would duplicate. Net-new storage is the
`net_settled` column only (~35–50 GiB projected — see measurements).

### 2. Read path — tx-keyed reads

The table's sort key is `asset_id`-leading; the transaction-list read fetches
by `(ledger, tx)` (a page of ~25 transaction ids), which is a scan against that
key. Wired through the existing shared list-aggregate fetch
(`fetch_tx_list_aggregates`), which already runs one keyed second query per page
for `operation_types` — the value read is a second aggregate there
(`max(net_settled)` per (tx, asset) + assets/decimals join).

> **Grounded — the metric is the network-flow FLOW VALUE, not an invention.**
> A devil's-advocate pass challenged `max(Σ+, Σ−)` as possibly under-counting
> (wash / round-trip → 0; offsetting multi-op payments). Research settled it: by
> the **flow decomposition theorem** every flow decomposes into source→sink paths
> plus cycles, a path contributes its flow and a **cycle contributes exactly
> zero**. So `gross = Σ path + Σ cycle` and `net = Σ path` — our figure. The
> challenged cases are therefore **definitional, not defects**: a wash IS a pure
> cycle (and a zero-balance cycle is precisely how the wash-trading literature
> _detects_ a wash), and two offsetting payments genuinely decompose into one
> path. Per-account netting is also the standard multilateral-netting algorithm of
> clearing houses (BIS). **Net over gross** because `net ≤ gross` always: net
> never overstates, gross inflates every routed payment (3 hops of 100 → 300) and
> routing is the common case while washes are rare. A gross column, if ever
> wanted, yields `wash volume = gross − net` for free. Pinned by
> `pure_cycle_is_zero_by_flow_decomposition` +
> `fan_out_through_a_partial_pass_through_hub` (a 5-edge DAG → 180).
> Refs: flow decomposition (Columbia IEOR6614 / MIT 15.082J), BIS netting
> schemes, arXiv 2102.07001 (wash-trading detection).

> **Implemented — `net_settled` is NULLABLE (fundamental fix).** `Nullable(Int128)`:
> `Some(0)` = genuinely nothing settled net; `NULL` = NOT COMPUTABLE (the reducer
> could not represent the result in i128 — a **bespoke** token's `ContractData`
> balance is contract-authored, i.e. attacker-chosen, so a multi-movement sum can
> overflow, and a SAC whose registry entry is missing is dropped). Keeping the two
> apart stops an uncomputable value from masquerading as a real zero. The value path
> honours it (`checked_sub`/`checked_abs`/`checked_add` → `None`), never a
> placeholder `0`. The read filters `IS NOT NULL AND != 0`
> and uses `assumeNotNull`: an aggregate over a Nullable column is `Nullable(T)`,
> and decoding that into a non-nullable Rust field 500s (the 0324 trap).

> **Decided (review) — NO version column; version-less RMT + `max(net_settled)`.**
> An earlier iteration made the table `ReplacingMergeTree(inserted_at)` so a
> re-ingest with a _corrected, even smaller_ value would win. Removed: (1) the
> engine change is un-`ALTER`-able and would force a full 9.5 B-row prod rebuild;
> (2) `net_settled` has a **single writer** — `stage.rs`, run by both live ingest
> and the full S3 re-ingest — so historical and live rows for a key are computed
> identically and there is never a second writer to disagree; (3) a downward
> correction of a deterministic figure only follows a change to our own reducer —
> a deploy event, handled by re-running the backfill + `OPTIMIZE FINAL`, not a
> runtime need worth a per-row version. The read dedups with `max(net_settled)`
> (`max` ignores NULL, so a computed value wins over a not-computed one per key).

> **Implemented — the projection idea was REVERSED.** A `(ledger, transaction_id)`
> projection is not viable: CH 26.3 refuses projections on a ReplacingMergeTree
> (Code 344; see `accounts_recent` / task 0353), and a `(ledger, tx)`-ordered copy
> would re-store the incompressible `transaction_id` (~85 GiB) anyway — the
> separate-table cost this table's design rejects. The read is a partition-pruned
> scan; NO access-path optimisation is baked in. Because this endpoint is polled
> (the `contract_ids` quota-outage shape, tasks 0243/0386), the right mechanism
> (`minmax` skip index vs an `accounts_recent`-style companion vs other) must come
> from a concrete load measurement — see Future Work.

### 3. Source & indexer — compute net per (tx, asset)

Compute per-account signed deltas per (tx, asset) and reduce to the net-settled
value. Cleanest source is **ledger-entry balance changes** (before/after),
which auto-net hops; the deferred version-safe `TransactionMeta` reader
(`meta.rs`, held in `stash@{0}` off the 0359 branch) is exactly this reader and
should be revived. Bespoke Soroban token balances live in contract storage
(`ContractData`) — decode those keys too. Combine:

The shipped reader is the **ledger-entry balance-delta** reader
(`ledger_value.rs`): before→after balances on `AccountEntry` / `TrustLineEntry` /
`ContractData`. It covers **every** flow uniformly — classic ops AND Soroban SAC /
bespoke-token transfers (contract-held balances live in `ContractData`) — and
auto-nets hops. Token events are **not** used for value (see the redesign note
below); there is no separate op-amount or event-amount path, and so nothing to
de-duplicate.

> **Redesign — value from the LEDGER, not events.** An earlier cut took the
> Soroban value from token-event `data`. Events are contract-emitted **logs** (any
> contract can emit any `"transfer"` it likes), so that value was spoofable. It was
> replaced by the ledger reader above: value for EVERY tx (classic and Soroban)
> comes from the authoritative `AccountEntry` / `TrustLineEntry` / `ContractData`
> balance changes, which a contract cannot forge. There is no `has_soroban` value
> split. The project-wide follow-up on this log-trust class is task 0415; the
> superseded event-guard is 0410.
>
> **History is uniform, not asymmetric.** Because value now reads `TransactionMeta`
> ledger changes for ALL txs, and those live only in S3 (not CH), ALL historical
> value — classic and Soroban alike — is recovered by the full S3 re-ingest (§4),
> never a CH-local pass. Live-forward is computed inline for both.

### 4. Backfill — value history comes from the full re-ingest

The value (classic AND Soroban) is computed only by `stage.rs`, run at live
ingest and by the **full S3 re-ingest**. There is **no CH-local value backfill**:
classic needs `TransactionMeta` (S3-only), and the Soroban value was folded into
the re-ingest too rather than duplicated in a separate script. Until the
re-ingest runs, historical `net_settled` is `NULL` (hidden by the read's
`HAVING net_settled IS NOT NULL`).

The 0383 token-flow backfill stays **presence-only** — it writes `net_settled:
NULL` and must **NOT** run after the column lands: its NULL row could win the
version-less RMT merge and blank a live-computed value for the same key.

### 5. Frontend — "X ASSET + N others"

Per (tx, asset) rows aggregate into a cell: the primary asset (`150 USDC`) plus
`+ N` for the other **assets** the tx moved (not transfers). Same figure on the
account list and the global list (it is a transaction-level intrinsic); only
display ordering may differ. (Primary-asset selection is Open Decision #1,
**decided native-first**: the read orders `asset_type` then `asset_id`, so
`values[0]` is XLM when the tx moved it. Known limitation: a tx moving dust XLM +
a large credit shows the dust as primary — see Open Decisions #1.)

## Acceptance Criteria

- [x] `operation_asset_appearances` carries `net_settled Nullable(Int128)` (raw),
      net-settled per (tx, asset) per the formula and its 3 rules. **NULLABLE** so
      "not computed yet" (backfill) stays distinct from a real net-zero.
      **`transfer_count` DROPPED** — "+ N others" is a read-time count of the
      asset rows per tx, not a stored column (see Implementation Notes).
- [x] Native XLM canonicalised to one `asset_id`; fee excluded (it is not in
      `TransactionMeta`); `max(Σ+, Σ−)` used (burns/redeems non-zero).
- [x] Value derived inline at **live ingest** from the **ledger** for ALL txs
      (classic + Soroban): `AccountEntry` / `TrustLineEntry` / `ContractData`
      balance deltas — never token events. (Live-forward only — history via the
      S3 re-ingest, below.)
- [ ] **Value history via the full re-ingest** (classic + Soroban), not a
      CH-local script. The 0383 token-flow backfill is **presence-only** — it does
      not compute `net_settled`. Historical values stay `NULL` until the re-ingest
      re-runs `stage.rs`. The backfill's `NULL` rows must not run after the column
      lands (they could win the version-less RMT merge and blank live values).
- [ ] **Classic history is NOT CH-local — needs the full S3 re-ingest.**
      `TransactionMeta` is not stored in CH, so classic historical values stay
      `NULL` until the planned re-ingest re-runs `stage.rs` over every ledger.
      Not a gap in this task's code; a deployment step (see Operations, step 3).
- [ ] ~~Projection makes the read a prefix seek~~ — **RULED OUT** (CH refuses
      projections on RMT; a `(ledger, tx)` copy re-stores ~85 GiB). The read is a
      partition-pruned scan; the access-path optimisation is deferred to a load
      measurement (Future Work).
- [ ] **RELEASE GATE — do not expose the value read on production polling until
      the scan cost is proven safe on a mature partition.** The value read scans
      the `asset_id`-leading `operation_asset_appearances` (~26M rows/page today,
      pruned only by the young head partition + the new `idx_oaa_transaction_id`
      bloom); it is the **global tx list**, the most-polled page, and this exact
      endpoint family has exhausted the read quota before (0243/0386). Gate: either
      the `(ledger, tx)`-leading companion table lands (accounts_recent pattern) OR
      a mature-partition load test confirms the scan + two `FINAL` joins stay within
      budget. `wants_values` already scopes the cost to the single global tx list —
      keep it there until this gate passes.
- [x] Read scales raw `Int128` by `decimals` at read time (no baked decimals;
      classic/SAC = 7, via the `soroban_contracts`→`soroban_contract_metadata`
      coalesce).
- [x] Frontend cell renders the primary asset + "+ N" on both the global and
      per-account transaction lists (`ValueCell`).
- [x] **Docs updated** — 5 `docs/architecture/**` files per ADR 0032.
- [x] **API types regenerated** — `TransactionValue` on the tx-list DTO;
      `libs/api-types/src/{openapi.json,generated/}` committed.

## Open Decisions (product)

1. **Primary-asset selection** for "X + N others" when a transaction moves
   several assets. **DECIDED: native-XLM-first.** The read orders `asset_type`
   first (native = type 0), so the frontend's first row is XLM when the tx moved
   it; credit assets follow by surrogate. (Without a price feed there is no
   "largest by value"; native-first matches the multi-instruction-chain
   precedent.) **Known UX limitation (re-review):** a tx moving dust XLM plus a
   large credit renders the dust as primary and collapses the significant asset
   into `+N`, which partly defeats "scan by amount". A price-free improvement —
   prefer a non-native asset when native is below a dust threshold, or pick the
   largest raw magnitude — is a candidate for the find-by-amount work (task 0408);
   confirm with product.
2. **Display vs sort/filter.** The originating request was to _find_ by amount,
   which implies sort/filter, not just display. **Decided: display-only MVP**;
   the value column renders per (tx, asset) but is not yet sortable/filterable. A
   genuine amount filter is deferred to **task 0408** (find-by-amount) — it needs
   an amount-oriented access path, folded into the read-path perf investigation.
3. **Account view semantic.** The stored figure is the transaction-total net
   (identical for both views). On complex multi-party transactions this differs
   from the viewing account's own delta. Measured to be a minority on classic
   accounts (see measurements); accepted as tx-level intrinsic. Revisit only if
   account-own-delta is wanted.

## Value source: the LEDGER, not events (redesign — supersedes the H2/0410 gate)

**A token event is a LOG** — any contract can emit any `"transfer"` topic it likes,
so an event's asset + amount are contract self-reports, not authoritative. Reading
value from events was the wrong source, and the SAC crypto-guard (former H2 / task 0410) was a patch around trusting logs.

**Value now comes exclusively from the authoritative LEDGER** — the balance-entry
changes consensus actually applied, which a contract cannot forge — for EVERY tx,
classic or Soroban:

The ledger reader emits the per-domain **`LedgerAsset`** enum — one variant per
balance-bearing ledger-entry type:

| holder / asset                  | ledger entry                                            | `LedgerAsset` variant                                                                       |
| ------------------------------- | ------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| account, native                 | `AccountEntry.balance`                                  | `Native`                                                                                    |
| account, classic credit         | `TrustLineEntry.balance`                                | `Credit { code, issuer }`                                                                   |
| contract, SAC (classic-wrapped) | `ContractData` `Balance`, SAC `BalanceValue` **struct** | `SacWrapped(C…)` → reversed to the wrapped classic `asset_id` via `sac_classic` (DB type 1) |
| any, bespoke token              | `ContractData` `Balance`, **bare `i128`**               | `Bespoke(C…)` → the token IS the asset (DB type 3)                                          |

`ledger_balance_deltas` telescopes `State`(before) → `Updated`(after) into signed
per-(holder, asset) deltas; `ledger_deltas_net_settled` (persist) resolves each
variant to its `asset_id` surrogate and reduces to `max(Σ+, Σ−)`. A SAC transfer's
contract leg (`SacWrapped`) and account leg (`Credit`/`Native`) resolve to the SAME
`asset_id`, so a mixed transfer **nets as one asset** — no double-count. All of this
is **cross-validated 1:1 on real mainnet data** (see [Verification](#verification--cross-validated-on-real-mainnet-data)).

**Two per-domain enums, not one shared type.** `LedgerAsset` (ledger reader) is
separate from **`EventAsset`** `{ Native, Credit, Bespoke }` (the event decoder,
presence-only). Each is total over its own producer's cases — no impossible `=>None`
arms, no boolean flag (the earlier shared `AssetIdentity`/`ContractHeld{bool}` was
split to match the project's one-enum-per-domain convention, cf. `AssetRef`,
`SacAssetIdentity`). Consolidating the ~3 parallel asset enums project-wide is task
[0418](../backlog/0418_REFACTOR_asset-vocabulary-consolidation-and-module-conventions.md).

**Deleted with this redesign:** the whole event-value path —
`token_events_net_settled`, `tx_token_net_settled`, the H2 gate, `policy_null`,
`sac_rejected`, `net_id` in the value path, and the 0410 SAC-crypto-guard-on-events.
`derive_token_event` stays — events remain a legitimate PRESENCE signal (who
participated), just never a value/amount source. This generalises: **[0415](../backlog/0415_AUDIT_authoritative-facts-ledger-not-logs.md)**
audits the whole indexer for other authoritative facts wrongly taken from logs.

## Verification — cross-validated on real mainnet data

The value pipeline was cross-checked **1:1 on real mainnet transactions** (2026-07-20)
against **3 independent derivations of the same authoritative meta**: our reader
(Rust), **Horizon `/effects`** (Go — incl. protocol-23 `contract_debited/credited`,
which resolve SAC→native/USDC exactly like our `sac_classic`), and the **`stellar`
CLI** XDR decode (hand-derived). Meta for any (incl. historical/edge) tx comes from
Soroban RPC `getTransaction` and **stellar.expert `/tx` `meta`** (unlimited retention).

Frozen as a gated regression corpus: `crates/xdr-parser/tests/net_settled_real_corpus.rs`
(8 fixtures) + `crates/db-clickhouse/tests/net_settled_real_e2e.rs` (end-to-end through
the production resolver). Dev harness: `crates/xdr-parser/examples/decode_meta.rs`.

| branch                                                                         | real-data                                                     | source                                     |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------- | ------------------------------------------ |
| Native / Credit / multi-asset                                                  | ✅ 1:1                                                        | Horizon                                    |
| telescoping / netting (path-payment, net<gross)                                | ✅ 1:1                                                        | Horizon                                    |
| **SacWrapped** (contract-held SAC)                                             | ✅ 1:1                                                        | Horizon contract-effects **+** stellar CLI |
| **Bespoke** (bare-i128 amount)                                                 | ✅ 1:1                                                        | stellar CLI decode                         |
| Created (create_account) · Removed (account_merge)                             | ✅ 1:1                                                        | Horizon                                    |
| failed-tx (moves nothing)                                                      | ✅ 1:1                                                        | Horizon                                    |
| Soroban mint (7 classic assets, issuer-side correctly absent)                  | ✅ 1:1                                                        | Horizon                                    |
| claimable-balance (pass-through **nets to 0** + 0413 issuer/CB gap, fail-safe) | ✅ 1:1                                                        | Horizon + stellar CLI                      |
| **prod net-merge** through `ledger_deltas_net_settled` + registry              | ✅                                                            | E2E test (4 legs → 2 assets)               |
| clawback · Restored · C→C Soroban                                              | ⚠️ unit only — rare on recent mainnet, needs S3/older ledgers | —                                          |
| adversarial (overflow / registry-miss / spoof)                                 | ⚠️ unit-tested only                                           | —                                          |

**Honest scope:** everything that carries real value on mainnet is cross-validated live;
the tail (clawback / Restored / C→C — rare in recent data, findable via S3/older ledgers)
and the failure modes (overflow, registry-miss→drop, spoofed-event→ignore — physically
hard to produce on mainnet) remain unit-tested. Not literally 100% of branches on real
data. (Verification also caught a false-alarm "discrepancy" on the CB tx that turned out
to be pass-through **netting working correctly** — confirmed by the independent CLI decode.) Note: Horizon
and stellar CLI both decode with the same `stellar-xdr` spec, so they are independent
**derivations** over one authoritative meta, not independent oracles of the raw fact.

## Remaining steps

**Code — DONE** (ledger redesign + `EventAsset`/`LedgerAsset` split + cross-validation);
all green (clippy `-D warnings`, unit + real-data corpus + prod E2E). Nothing
code-blocking left in 0393 itself.

**A. Commit / merge (needs explicit go — currently uncommitted):**

1. Regen api-types (`nx run @rumblefish/api-types:generate`) — `crates/api/**` +
   `dto.rs` changed → the freshness gate needs `openapi.json` + `generated/*` staged.
2. Commit + open/update PR (base develop). Move task 0410 → `archive/` and 0393 →
   `archive/` on merge; the 0411–0418 backlog tasks land on develop.

**B. Deployment / Operations → MOVED to [[0419]]** (OPS: prod rollout +
post-reingest verification). The ordered prod steps now LIVE there — work from
0419, not this archived copy. Summary for the record: 3. `ALTER … ADD COLUMN net_settled Nullable(Int128)` (before the new indexer deploys). 4. Deploy the new indexer (live-forward values start). 5. Full **S3 re-ingest** to fold value into history (classic + Soroban; no CH-local backfill). 6. Confirm `assets.id` backfilled (else the value read's `INNER JOIN assets` drops rows). 7. Add + `MATERIALIZE` the `idx_oaa_transaction_id` bloom skip index. 8. **RELEASE GATE** — do NOT expose the value read on prod polling until task **0417**
(the `(ledger,tx)` companion) lands OR a mature-partition load test clears the scan.

**C. Verification tail (optional — raise coverage from "main paths" to "full"):** 9. claimable-balance is now in the corpus (✅). Still pending: **clawback** and
**C→C Soroban** — rare on recent mainnet (0 found in 800 events); need S3/older
ledgers or a targeted clawback-asset search. Restored + adversarial stay unit-only
(inherent — can't produce on mainnet).

**D. Follow-up tasks (backlog, NOT blocking 0393):** 0411 (detail page + F) · 0412 · 0413 ·
0414 · 0415 (NFT-ownership first) · 0416 · 0417 (release gate) · 0418 · 0408.

## Operations — do AFTER the code lands, before/at deploy

> **Owner: [[0419]]** — these steps were lifted into that OPS task (with a required
> post-S3-reingest cross-validation step). Kept here as the source-of-record; run
> the rollout from 0419.

These are prod-database steps, not code. `init.sql` reflects the desired schema;
prod is migrated by hand. **Ordering matters** — the indexer INSERTs the new
`net_settled` column, so the column must exist on prod before the new indexer
deploys, or every `operation_asset_appearances` insert fails `Code 16: No such
column` and ingestion halts.

1. **Add the column (additive, no rebuild).** The engine is version-less
   `ReplacingMergeTree` (the version idea was dropped precisely so this stays a
   plain `ALTER`), so:
   ```sql
   ALTER TABLE operation_asset_appearances ADD COLUMN net_settled Nullable(Int128);
   ```
   Existing rows read `NULL` until re-written — correct ("not computed"), and the
   read's `HAVING IS NOT NULL` hides them, so no wrong value shows pre-backfill.
2. **Deploy the new indexer** (now that the column exists) so live ingest starts
   writing values forward.
3. **Fold values into history — the full S3 re-ingest.** Value (classic AND
   Soroban) is recomputed only by re-running `stage.rs` over every ledger; there
   is no CH-local value backfill (`TransactionMeta` is S3-only). Until it runs,
   historical `net_settled` stays `NULL` (hidden by `HAVING IS NOT NULL`).
   **Do NOT run the 0383 token-flow backfill after this column exists** — it is
   presence-only and writes `net_settled: NULL`, which could win the version-less
   RMT merge and blank a live-computed value.
4. **`assets.id` must be backfilled** (its own pending prod step, per the `assets`
   note in `init.sql`): the value read does `INNER JOIN assets ON a.id = asset_id`,
   and un-backfilled `assets` rows have `id = 0` and join nothing → those values
   silently vanish. Confirm `assets.id` is populated for the range before relying
   on the column.
5. **Add + materialize the bloom skip index (value-read mitigation).** `init.sql`
   now declares `idx_oaa_transaction_id` on `operation_asset_appearances`; prod
   needs it added and built over existing parts:
   ```sql
   ALTER TABLE operation_asset_appearances
     ADD INDEX idx_oaa_transaction_id transaction_id TYPE bloom_filter(0.001) GRANULARITY 1;
   ALTER TABLE operation_asset_appearances MATERIALIZE INDEX idx_oaa_transaction_id;
   ```
   `MATERIALIZE` is a background mutation over existing parts; until it finishes,
   only new parts are granule-pruned. Turns the `(ledger, tx)` filter's ~26M-row
   partition scan into a ~10x-smaller pruned read. RMT-safe.

> **`max()` is a one-way ratchet (re-review).** The read dedups with
> `max(net_settled)`, so if a future reducer fix produces a _smaller_ correct
> value, the stale larger row keeps showing until the old rows physically merge
> away. A corrective re-backfill therefore is NOT enough on its own — budget an
> `OPTIMIZE TABLE operation_asset_appearances FINAL` (expensive on 9.5 B rows)
> over the affected partitions, and know that until it completes the correction
> is invisible.

## Read-path performance — mitigated (bloom skip index + D1 de-scoping)

Measured: the value read is NOT a primary-key seek — the table is `asset_id`-leading
and we filter `(ledger, tx)`, so unaided it SCANS the pruned partition (~26 M
rows/page on a full partition, vs ~16 k for the seek-based op-types query beside it),
plus three **un-pruned** dimension joins (`assets.id` is not in its ORDER BY). This
endpoint family is polled and previously exhausted the CH read quota in exactly this
shape (tasks 0243/0386). The **account** list was the worst case: its page spreads
across many ledger-partitions, so the scan is **unbounded** (partition-count × 26 M),
not capped like the single-partition ledger/global page.

**Mitigations applied in-branch:**

- **`wants_values` gate + D1 de-scoping.** `fetch_tx_list_aggregates` takes a
  `wants_values` flag; the LP + asset lists (op-types only) always passed `false`.
  Per **decision D1**, the **account** list (unbounded worst case) and the **ledger
  detail** list (never rendered the column) now also pass `false`. Only the **global
  tx list** pays the value read today.
- **Bloom skip index.** `init.sql` declares `idx_oaa_transaction_id` (bloom on
  `transaction_id`, same pattern as `idx_oa_contract_id` / `idx_acc_id`); it prunes
  granules holding none of a page's tx_ids (~10×), turning the partition scan into a
  granule-pruned read. RMT-safe (only projections are refused, CH Code 344). Prod
  migration = Operations step 5 above.

**Fallback — only if the bloom proves insufficient at scale:**

- A `(ledger, tx)`-ordered companion (the `accounts_recent` pattern — plain
  MergeTree + refreshable MV + atomic `EXCHANGE`; a projection is refused on an RMT,
  CH Code 344) so the read is a true seek. Its own prod migration; defer until a load
  measurement shows the bloom is not enough.
- Reduce the un-pruned joins: `decimals = 7` for classic/SAC, so the metadata joins
  are only needed for bespoke type-3.
- Re-enabling the account/ledger columns (task **0411**) is gated on this being cheap.

Tracked with task **0408** (find-by-amount needs the same access path).

## Re-review — accepted low-severity notes

- **Bespoke type-3 with no metadata yet → decimals default to 7.** For a bespoke
  fungible token that has an `assets` row but no `soroban_contract_metadata` row
  during the enrichment window, the read's `coalesce(m.decimals, 7)` mis-scales
  the display (raw value is stored correctly; self-corrects when metadata lands).
  Consistent with the existing `total_supply` / `balances` read pattern.
- **Live vs history input-set equality — now a non-issue.** Value has a SINGLE
  source: `ledger_balance_deltas` over the tx's `TransactionMeta`, run by `stage.rs`
  at both live ingest and the full S3 re-ingest. Both consume the same authoritative
  meta with the same reducer, so live and historical rows for a key are computed
  identically — there is no live-events-vs-stored-events divergence (that concern
  belonged to the deleted event-value path).

## Future Work

> **Spawned backlog tasks (from this scope):**
>
> - **[0410](../backlog/0410_BUG_sac-event-identity-guard-on-value-path.md)** —
>   SUPERSEDED (the event-value guard; value now reads the ledger). Archive on merge.
> - **[0411](../backlog/0411_FEATURE_net-settled-detail-page-and-remaining-tx-tables.md)** —
>   tx-detail per-asset breakdown + remaining tx-list tables; **owns finding F**
>   (`fetch_tx_op_types` / `fetch_tx_values` split, drop `wants_values`).
> - **[0412](../backlog/0412_BUG_net-settled-undeclared-moved-asset.md)** — value
>   dropped for an asset the ledger moved but no op/event declared (Soroban-no-event).
> - **[0413](../backlog/0413_BUG_net-settled-issuer-side-cb-lp-own-asset.md)** —
>   issuer-side claimable-balance / LP of the issuer's own asset understates.
> - **[0414](../backlog/0414_REFACTOR_split-stage-god-module.md)** — split
>   `stage.rs` god module. **[0418]** adds `state.rs` (3290 LOC) as its twin.
> - **[0415](../backlog/0415_AUDIT_authoritative-facts-ledger-not-logs.md)** —
>   project-wide audit: every authoritative fact from the ledger, not logs
>   (NFT ownership is the first still-live target).
> - **[0416](../backlog/0416_PERF_soroban-events-fullcontent-storage-vs-readtime.md)** —
>   `soroban_events` storage (#1 table, 223 GiB) vs read-time decode (ADR 0044 Q6).
> - **[0417](../backlog/0417_PERF_net-settled-value-read-ledger-tx-companion.md)** —
>   `(ledger,tx)`-leading companion so the value read is a seek (the RELEASE GATE below).
> - **[0418](../backlog/0418_REFACTOR_asset-vocabulary-consolidation-and-module-conventions.md)** —
>   consolidate the parallel asset enums in `domain` + module-conventions ADR + `state.rs` split.
> - **[0408](../backlog/0408_FEATURE_find-by-amount.md)** — find-by-amount:
>   sort/filter by value moved (the origin request's other half).

- Read-path performance — see the dedicated section above (measured; the
  `(ledger, tx)` companion is required before scale).
- USD-denominated volume (sum across assets by price) is blocked on the Prices
  API (task 0247); this task deliberately stays asset-native. When prices land,
  a USD figure is a read-time join on top of the stored raw amounts. Spawn a
  follow-up backlog task at that point.
- **Hygiene (low priority, from the 0393 reuse audit)** — not blockers, noted so
  they are not lost:
  - `ledger_value.rs` and `ledger_entry_changes.rs` both decode the two balance
    carriers (`AccountEntry.balance`, `TrustLineEntry.balance` + trustline asset).
    ~15 lines of match arms overlap. Output types legitimately differ (typed
    deltas vs detail-page JSON) and `ledger_value` adds the before→after
    telescoping/netting on top, so consolidation is low-value — but a shared
    `(account, asset_sep11, balance)` extractor could back both.
  - `ledger_entry_changes.rs` still has its own `TransactionMeta::V3/V4` match
    instead of going through `meta.rs` (`located_ledger_changes`) — one of the
    exact wildcards the 0359 `meta.rs` "adoption" was meant to strangle (0359
    README). Migrating it closes that gap and would let both modules share the
    meta walk.
  - ~~Ingest parses each Soroban event twice (presence + amount)~~ — **RESOLVED by
    the redesign.** The event-amount decode (`token_event_amount`) is deleted, so
    events are parsed once (presence only, `derive_token_event`); value comes from
    the ledger, not a second event pass.
  - The surrogate credit formula was consolidated into `ids::credit_asset_id`
    (task 0393) — all six production call sites now share it; the golden test
    `credit_asset_id_matches_raw_formula` pins the equivalence.
