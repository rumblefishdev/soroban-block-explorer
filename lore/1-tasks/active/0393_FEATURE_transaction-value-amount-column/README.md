---
id: '0393'
title: 'FEATURE: transaction value ("amount moved") column — net-settled per-asset value for tx-list views'
type: FEATURE
status: active
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
  - crates/xdr-parser/src/event_filters.rs
  - crates/backfill-runner/src/soroban_token_flow_backfill.rs
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
> could not represent the result in i128 — amounts are contract-emitted, i.e.
> attacker-chosen — or a recognised event's amount was unreadable). Keeping the
> two apart stops an uncomputable value from masquerading as a real zero. Both the
> live path and the backfill honour it: any un-representable / unreadable reduction
> writes `NULL`, never a placeholder `0`. The read filters `IS NOT NULL AND != 0`
> and uses `assumeNotNull`: an aggregate over a Nullable column is `Nullable(T)`,
> and decoding that into a non-nullable Rust field 500s (the 0324 trap).

> **Decided (review) — NO version column; version-less RMT + `max(net_settled)`.**
> An earlier iteration made the table `ReplacingMergeTree(inserted_at)` so a
> re-ingest with a _corrected, even smaller_ value would win. Removed: (1) the
> engine change is un-`ALTER`-able and would force a full 9.5 B-row prod rebuild;
> (2) the live indexer and the 0383 backfill now reduce the SAME value from the
> SAME events (shared `token_events_net_settled`), so both writers emit an
> identical row and there is nothing to correct between them; (3) a downward
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

- classic operation/result amounts (0359 path) — covers classic across all
  history, terminal amounts (no hop inflation), and pre-CAP-67 history.
- Soroban token-event amounts (0383 path) — read the amount out of `data_xdr`
  (already stored; a **CH-local transform, no S3 re-parse** — the same pattern
  as the existing `nft_reparse` and the 0383 backfill).

The op-path/event-path split already de-duplicates classic vs Soroban (0383 is
scoped to `has_soroban`); the same split prevents double counting here.

> **Implemented:** the classic path shipped as the **ledger-entry balance-delta**
> reader (`classic_value.rs`), NOT op/result amounts — it covers every classic op
> type uniformly (payment, path payment, offer, LP, claimable balance, clawback)
> and auto-nets, where op-amounts could not cover LP/CB-claim without the ledger
> changes. Soroban stays on the token-event path. The two are routed by
> `has_soroban` (protocol forbids mixing the op kinds in one tx, so no
> double-count). **Diagnostic-source events are filtered** on the Soroban path
> (they carry byte-identical copies that would double the amount).
>
> **Historical coverage is asymmetric.** Only `soroban_events` (`topics_xdr` /
> `data_xdr`) is stored in ClickHouse, so ONLY the Soroban token-event value can
> be backfilled CH-local. The classic path reads `TransactionMeta` ledger
> changes, which are **not in CH — they live only in S3**; classic history
> therefore CANNOT be recovered by a CH-local pass. It is filled by the **planned
> full re-ingest from S3** (which re-runs `stage.rs` over every ledger and so
> recomputes both classic and Soroban values). Live-forward is CH-local for both.

### 4. Backfill — two mechanisms, not one

- **Soroban token-event value → CH-local**, folded into the pending 0383
  backfill (`data_xdr` is stored; `token_events_net_settled` reduces it, same fn
  as live). One pass, no second scan of billions of events.
- **Classic value → full re-ingest from S3.** `TransactionMeta` is not in CH, so
  there is no CH-local route; classic history is `NULL` until the re-ingest runs
  (the read's `HAVING net_settled IS NOT NULL` hides it meanwhile). That
  re-ingest also recomputes the Soroban value, making the 0383 amount backfill
  redundant for the column — the 0383 backfill retains its own presence-row job.

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
- [x] Value derived CH-local at **live ingest**: classic from ledger-entry
      balance deltas, Soroban from the token-event `data`; no S3 re-parse.
      (Live-forward only — see history caveat below.)
- [x] **Soroban history folds in CH-local (G1, done).** The 0383 backfill reads
      `data_xdr` and reduces with the SAME `token_events_net_settled` the live
      indexer runs, so it emits identical rows. This is required, not optional:
      the table is a `ReplacingMergeTree`, so a placeholder-`NULL` backfill row
      could win over a live-computed value on merge and blank the column.
      (Running it on prod is the OPS step below.)
- [ ] **Classic history is NOT CH-local — needs the full S3 re-ingest.**
      `TransactionMeta` is not stored in CH, so classic historical values stay
      `NULL` until the planned re-ingest re-runs `stage.rs` over every ledger.
      Not a gap in this task's code; a deployment step (see Operations, step 3).
- [ ] ~~Projection makes the read a prefix seek~~ — **RULED OUT** (CH refuses
      projections on RMT; a `(ledger, tx)` copy re-stores ~85 GiB). The read is a
      partition-pruned scan; the access-path optimisation is deferred to a load
      measurement (Future Work).
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

## Event-value trust gate (H2) — spoof closed here, full guard in 0410

A token event's trailing SEP-11 asset string (`"USDC:GISSUER…"`) is NOT
cryptographically bound to the emitter, so a hostile contract could emit one and
forge a classic-asset amount. **Closed in this task (interim):** on the Soroban
value path, only a bespoke `Contract` identity — which IS the emitter, hence
unspoofable — is attributed a value; a `Native`/`Credit` claim from events is
written as `NULL` ("not computed"), so a spoof renders a dash, never a fabricated
figure (`token_events_net_settled`, gate + tests
`credit_claim_from_an_unverified_emitter_is_not_attributed_a_value` /
`native_claim_from_events_is_also_untrusted`).

**Cost of the interim:** a _genuine_ SAC transfer of a classic asset inside a
Soroban tx also loses its event-derived value (dash) until the full guard lands.
The classic path (`has_soroban = 0`) is unaffected — it reads ledger balance
deltas, unspoofable. **[0410](../backlog/0410_BUG_sac-event-identity-guard-on-value-path.md)**
recovers that coverage by verifying `emitter == derive_sac(asset)` with the
existing crypto-match `sac_override_from_event_topics` (task 0294) — a
cross-cutting change (`net_id` into `StageInputs` + fixture rework), hence its own
task.

## Operations — do AFTER the code lands, before/at deploy

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
3. **Fold values into history — two mechanisms:**
   - **Soroban:** run the 0383 token-flow backfill (CH-local; reduces the same
     value live does, so its rows agree with live's — safe to overlap).
   - **Classic:** covered ONLY by the **full S3 re-ingest** (`TransactionMeta` is
     not in CH). Until that runs, classic historical values stay `NULL` (hidden by
     `HAVING IS NOT NULL`). The re-ingest also recomputes the Soroban value, so if
     it runs first the 0383 amount backfill is redundant for this column.
4. **`assets.id` must be backfilled** (its own pending prod step, per the `assets`
   note in `init.sql`): the value read does `INNER JOIN assets ON a.id = asset_id`,
   and un-backfilled `assets` rows have `id = 0` and join nothing → those values
   silently vanish. Confirm `assets.id` is populated for the range before relying
   on the column.

> **`max()` is a one-way ratchet (re-review).** The read dedups with
> `max(net_settled)`, so if a future reducer fix produces a _smaller_ correct
> value, the stale larger row keeps showing until the old rows physically merge
> away. A corrective re-backfill therefore is NOT enough on its own — budget an
> `OPTIMIZE TABLE operation_asset_appearances FINAL` (expensive on 9.5 B rows)
> over the affected partitions, and know that until it completes the correction
> is invisible.

## Read-path performance — REQUIRED before this ships at scale (deferred)

Measured, not hypothetical: the value read SCANS the pruned partition (~26 M
rows/page against a full partition, vs ~16 k for the seek-based op-types query
beside it), because the table is `asset_id`-leading and we filter `(ledger, tx)`.
Plus three **un-pruned** dimension joins (`assets.id` is not in its ORDER BY). This
endpoint family is polled and previously exhausted the CH read quota in exactly
this shape (tasks 0243/0386) — the cost is hidden today only because the head
partition is young (~4% full); it grows ~43,000× as the partition fills over ~4
weeks, and the **account** transactions list pages into full partitions already.

**Done in-branch (re-review):** the "cheap now" caller split — `fetch_tx_list_
aggregates` gained a `wants_values` flag; the LP + asset transaction lists (which
render only `operation_types`) pass `false` and skip the value scan + joins
entirely. Only the 3 endpoints that render the column pay.

Remaining fixes, increasing effort:

- **Cheap now (pure code):** `fetch_tx_list_aggregates` has 5 callers but only 3
  render `values` (`transactions`, `accounts`, `ledgers`); `liquidity_pools` and
  `assets` pay the scan + joins and discard it. Split `fetch_tx_list_value_moved`
  out and call it only where the column is rendered.
- **The real fix:** a `(ledger, tx)`-ordered companion (the `accounts_recent`
  pattern — plain MergeTree + refreshable MV + atomic `EXCHANGE`; a projection is
  refused on an RMT, CH Code 344) so the read is a seek. This is its own prod
  migration — do it in the same window as the Operations steps above.
- Reduce the un-pruned joins: `decimals = 7` for classic/SAC, so the metadata
  joins are only needed for bespoke type-3.

Tracked with task **0408** (find-by-amount needs the same access path).

## Re-review — accepted low-severity notes

- **Bespoke type-3 with no metadata yet → decimals default to 7.** For a bespoke
  fungible token that has an `assets` row but no `soroban_contract_metadata` row
  during the enrichment window, the read's `coalesce(m.decimals, 7)` mis-scales
  the display (raw value is stored correctly; self-corrects when metadata lands).
  Consistent with the existing `total_supply` / `balances` read pattern.
- **Live vs backfill input-set equality is near-total, not proven.** Live reduces
  over parser events (filtered only by `is_diagnostic`); the backfill reduces over
  stored `soroban_events`, which the writer already dropped orphan (contract-id-less)
  events from. A token event with no emitting contract id would differ — but such
  an event resolves to no asset identity on both paths anyway (bespoke) or is
  untrusted→NULL (classic, H2 gate), so no real divergence has been constructed.

## Future Work

> **Spawned backlog tasks (on develop):**
>
> - **[0409](../backlog/0409_REFACTOR_arm-a-nft-pollution-separation.md)** —
>   arm-A NFT pollution: root cause (done) + pick a permanent fungible/NFT
>   separation strategy. The ingest-time NFT gate is one candidate there.
> - **[0408](../backlog/0408_FEATURE_find-by-amount.md)** — find-by-amount:
>   sort/filter transactions by value moved (the origin request's other half).

- Read-path performance — see the dedicated section above (measured; the
  `(ledger, tx)` companion is required before scale).
- USD-denominated volume (sum across assets by price) is blocked on the Prices
  API (task 0247); this task deliberately stays asset-native. When prices land,
  a USD figure is a read-time join on top of the stored raw amounts. Spawn a
  follow-up backlog task at that point.
- **Hygiene (low priority, from the 0393 reuse audit)** — not blockers, noted so
  they are not lost:
  - `classic_value.rs` and `ledger_entry_changes.rs` both decode the two balance
    carriers (`AccountEntry.balance`, `TrustLineEntry.balance` + trustline asset).
    ~15 lines of match arms overlap. Output types legitimately differ (typed
    deltas vs detail-page JSON) and `classic_value` adds the before→after
    telescoping/netting on top, so consolidation is low-value — but a shared
    `(account, asset_sep11, balance)` extractor could back both.
  - `ledger_entry_changes.rs` still has its own `TransactionMeta::V3/V4` match
    instead of going through `meta.rs` (`located_ledger_changes`) — one of the
    exact wildcards the 0359 `meta.rs` "adoption" was meant to strangle (0359
    README). Migrating it closes that gap and would let both modules share the
    meta walk.
  - Ingest parses each Soroban event through `parse_token_event` twice — once for
    presence (`derive_token_event`), once for the amount (`token_event_movement`).
    Negligible on live per-ledger volume; a single decode pass yielding both would
    matter only at backfill scale.
  - The surrogate credit formula was consolidated into `ids::credit_asset_id`
    (task 0393) — all six production call sites now share it; the golden test
    `credit_asset_id_matches_raw_formula` pins the equivalence.
