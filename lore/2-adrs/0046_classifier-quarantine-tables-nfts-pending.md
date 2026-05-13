---
id: '0046'
title: 'Classifier quarantine tables: nfts_pending / nft_ownership_pending'
status: accepted
deciders: [stkrolikiewicz]
related_tasks: ['0118', '0217']
related_adrs: ['0027', '0030', '0044']
tags:
  [schema, quarantine, nfts, contract-classification, indexer, persist-routing]
links: []
history:
  - date: 2026-05-13
    status: accepted
    who: stkrolikiewicz
    note: 'ADR created post-factum alongside PR #180 (task 0217 implementation).'
  - date: 2026-05-13
    status: accepted
    who: stkrolikiewicz
    note: >
      Same-day amendment: Alternative 4 (Patch C parser-only whitelist
      from PR #178) flipped from "ACCEPTED AS COMPLEMENT" to "REJECTED"
      after the pre-audit re-test discovered a real mainnet SEP-39 NFT
      (Bachini `CDA5FGE4...`) using `i128` token_id — Patch C would have
      silently dropped it. Patch C was reverted in the same branch as
      this PR; the parser is back to its permissive blacklist. The
      quarantine pattern + WASM-spec-based classifier remain the
      authoritative discrimination layer. Context and Alternative 4
      rewritten to reflect the empirical evidence.
---

# ADR 0046: Classifier quarantine tables for NFT-candidate rows

**Related:**

- [Task 0118: NFT false positives from fungible token transfers](../1-tasks/blocked/0118_BUG_nft-false-positives-fungible-transfers.md) — parent / parser-side fix (Patch C, PR #178)
- [Task 0217: PG+CH nfts_pending quarantine](../1-tasks/active/0217_FEATURE_nfts-quarantine-table.md) — this implementation
- [ADR 0027: Post-surrogate schema](./0027_post-surrogate-schema-and-endpoint-realizability.md) — hot `nfts` / `nft_ownership` definitions
- [ADR 0030: Surrogate `soroban_contracts.id BIGINT`](./0030_soroban-contracts-surrogate-bigint-id.md) — `contract_id` FK contract that quarantine deliberately omits
- [ADR 0044: ClickHouse pilot parallel store](./0044_clickhouse-pilot-parallel-store.md) — CH-side counterpart of the quarantine tables

---

## Context

The persist-time NFT-candidate filter shipped in task 0118 Phase 2
(PR #110) classifies every contract referenced by a parser-emitted
NFT row via `soroban_contracts.contract_type` and acts on the verdict:

| Verdict (pre-0217)   | Persist action                                                                   |
| -------------------- | -------------------------------------------------------------------------------- |
| `Nft` (=2)           | INSERT into `nfts` / `nft_ownership`                                             |
| `Fungible` / `Token` | drop                                                                             |
| `Other` (=1) / NULL  | INSERT into `nfts` / `nft_ownership` (**permissive — temporary false positive**) |

The permissive path was the only known way to avoid losing legitimate
NFT rows for contracts whose WASM upload hadn't yet been observed by
the indexer. The plan was that a post-backfill SQL cleanup (task 0118
Phase 3) would later drop the `Fungible`/`Token` rows once the
backfill had populated every WASM verdict.

The 2026-05-12 ClickHouse pilot endpoint audit measured the cost of
this design on real mainnet data
([`docs/audits/2026-05-12-ch-pilot-endpoint-audit.md`](../../docs/audits/2026-05-12-ch-pilot-endpoint-audit.md)
§E15–E17):

- 663 282 `nfts` rows across a 15.7k-ledger window — **99.4% of them
  misclassified fungible transfers**. XLM SAC (`CAS3J7GY…`) alone
  contributed 421 871 rows.
- `/v1/nfts*` endpoints returned garbage in production-like load
  because they read the hot tables directly.

PR #178 (task 0118 Patch C) initially narrowed the parser-side
`token_id` type whitelist (reject `i128`/`u128`, accept SEP-50 +
OpenZeppelin canonical shapes), eliminating the bulk of the
misclassifications at the source. **Patch C was subsequently
reverted** (PR #180, same branch as this ADR's implementation)
after a 2026-05-13 pre-audit re-test against live mainnet RPC
revealed a real SEP-39 NFT
(`CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY` /
James Bachini SorobanNFT) using `i128` for `token_id` — the
whitelist would have silently dropped a legitimate NFT collection.
See Alternative 4 below for the full rationale.

With Patch C reverted, the parser is back to its pre-2026-05-12
permissive blacklist (`!void|map|vec|error`). The architectural
problem this ADR addresses therefore stands unchanged: NFT-shape
events from contracts whose WASM has not yet been observed parse as
valid candidates, and we cannot decide their classification until
the WASM upload is observed.

The decision-shape problem: the `Other`/NULL bucket continues to
exist by design, and shipping NULL-classified rows into the hot
tables continues to expose dirty data to `/v1/nfts*` for the
duration of the backfill.

---

## Decision

Introduce a **classifier quarantine pattern**: dedicated `_pending`
companion tables that mirror the hot tables' row shapes and absorb
the `Other`/NULL bucket. Promotion / drop is wired into the existing
`reclassify_contracts_from_wasm` UPDATE so the verdict flip and the
row migration are atomic from any reader's perspective.

Implemented in PR #180 (task 0217) + task 0220 (CH writer parity):

- **Postgres:** full implementation — schema migration
  ([`crates/db/migrations/20260513130000_nfts_pending_quarantine.up.sql`](../../crates/db/migrations/20260513130000_nfts_pending_quarantine.up.sql))
  plus writer-side routing (`resolve_nft_filter` returns 4 buckets,
  12c/12d INSERTs, promotion hook in `reclassify_contracts_from_wasm`).
- **ClickHouse:** **full writer implementation (task 0220)** —
  `nfts_pending` + `nft_ownership_pending` are populated by
  `crates/db-clickhouse/src/persist/stage.rs::prepare`. The CH stage
  builds the per-ledger `wasm_classification` map alongside
  `wasm_interface_metadata` (same path as PG `Staged::prepare`) and
  routes NFT-candidate rows: `Nft` verdict → hot bucket; `Fungible` /
  `Token` → drop; `Other` / uncached (no interface in this ledger) →
  pending bucket. Writer `crates/db-clickhouse/src/persist/writer.rs`
  carries `Insert<NftPendingRow>` + `Insert<NftOwnershipPendingRow>`
  slots that lazy-open only on partitions containing at least one
  `Other`-classified contract. The promotion-hook atomicity gap that
  motivated the original schema-only carve-out is bridged via
  **re-emission on next observation** (when the WASM upload lands in
  a later ledger, the next event from the same contract is staged
  with the definitive verdict and routes to hot) plus the
  post-backfill drain runbook for stragglers.

Routing per classifier verdict (post-0217):

| Verdict              | Target                                                                    |
| -------------------- | ------------------------------------------------------------------------- |
| `Nft` (=2)           | `nfts` + `nft_ownership` (hot — API-facing)                               |
| `Fungible` / `Token` | _none_ (filter drop, unchanged from 0118)                                 |
| `Other` (=1) / NULL  | `nfts_pending` + `nft_ownership_pending` (quarantine — never read by API) |

Promotion semantics:

- `Other → Nft` — `promote_pending_nfts_to_hot` copies pending rows
  to the hot tables via column-projection INSERT (mirrors 12a's
  watermark-guarded upsert + 12b's natural-key join), then DELETEs
  the source rows. Same transaction as the
  `reclassify_contracts_from_wasm` UPDATE.
- `Other → Fungible`/`Token` — `drop_pending_nfts_for_contracts`
  DELETEs the pending rows without an intermediate hot INSERT (they
  were never NFTs).

Schema choices documented in the
[implementation task notes](../1-tasks/active/0217_FEATURE_nfts-quarantine-table.md):

- **No FKs** on quarantine tables (`soroban_contracts`, `accounts`,
  `transactions`, `nfts`) — rows arrive before classification, no
  read-side payoff for the FK churn on a by-design transient row.
- **Natural-key PKs** — `(contract_id, token_id)` and
  `(contract_id, token_id, created_at, ledger_sequence, event_order)`
  let promotion be a column-projection `INSERT INTO nfts SELECT …`
  without resolving a hot-side SERIAL ID first.
- **No partitioning on PG** — pending is transient; the by-`created_at`
  range pattern that drives `nft_ownership`'s partitioning has no
  read-side payoff here. CH keeps `intDiv(ledger_sequence, 500000)`
  on the ownership pending table for part-copy symmetry with
  `nft_ownership`.
- **Minimal indexing** — single `(contract_id)` btree per PG table;
  CH `ORDER BY` covers the read pattern. Pending is write-heavy and
  only read at promotion / drain time.

API endpoints never read the `_pending` tables. Production
`/v1/nfts*` becomes clean by design.

---

## Rationale

Two factors drove the quarantine pattern over alternatives:

1. **Hot-table cleanliness is a read-side property.** Any design that
   leaves NULL-classified rows in `nfts` until a post-hoc cleanup
   exposes those rows to every API request in the meantime. For a
   long-tailed backfill (Soroban-era = ~10M ledgers, multi-day
   compute), the "meantime" is the entire backfill window. The
   measured 99.4% false-positive rate in the audit is the worst-case
   manifestation of this design.

2. **Promotion is wirable atomically into the existing reclassify
   path.** `reclassify_contracts_from_wasm` already exists and
   already runs inside the persist transaction; adding the
   promote/drop step costs one extra SELECT (`SELECT id FROM
soroban_contracts WHERE wasm_hash = ANY(…)`) per ledger plus the
   actual row migration. There is no architectural change in
   _where_ verdict flips happen — only in _what else_ happens on
   the same UPDATE.

The `_pending` shape is a **reusable pattern**, not a one-off. Any
future classifier-pending data (e.g. asset-type-pending rows for SAC
contracts whose authoritative classic-asset mapping arrives late,
liquidity-pool-pending rows for pre-window pool deployments) can
follow the same template: pending table + persist-time routing
decision + atomic promotion hook in the relevant UPDATE path.

---

## Alternatives Considered

### Alternative 1: Keep permissive insert + post-backfill cleanup SQL (status quo pre-0217)

**Description:** Continue inserting `Other`/NULL rows directly into
the hot tables. Sweep the wrong ones via SQL after the full
Soroban-era backfill populates WASM verdicts (the 0118 Phase 3 plan,
unchanged).

**Pros:**

- Zero schema change.
- Promotion is implicit (the row is _already_ in the hot table).

**Cons:**

- Production `/v1/nfts*` reads dirty data for the entire backfill
  window (~days). The audit measured this empirically at 99.4%
  garbage.
- Cleanup SQL needs to run twice in the same window if the backfill
  doesn't complete in one shot — operational footgun.
- Couples the user-visible cleanliness of `/v1/nfts*` to the
  completion of an unrelated backfill operation.

**Decision:** REJECTED — the read-side cost is not acceptable for an
API-facing surface, regardless of what the cleanup SQL eventually
does.

### Alternative 2: Single-table design with an `is_pending BOOL` flag column

**Description:** Add a `pending` column to `nfts` /
`nft_ownership`. Routing writes the flag; API filters `WHERE NOT pending`.

**Pros:**

- No new tables.
- Promotion is a `UPDATE … SET pending = FALSE`.

**Cons:**

- Every API query gains a `WHERE NOT pending` clause — query plans
  shift; missed-WHERE bugs become silent data-quality regressions.
- Existing indexes don't help — need partial indexes
  (`WHERE NOT pending`) on every read path, doubling index storage.
- API surface couples to internal classification state. A future
  schema reader (BI, dashboard) sees `pending` rows by default
  unless they read the documentation.
- On CH side, `ReplacingMergeTree` semantics make a `pending` flag
  awkward to track because the version slot is already taken by
  `current_owner_ledger` / `last_updated_ledger`.

**Decision:** REJECTED — couples internal classification state to
the API surface and complicates the read path indefinitely.

### Alternative 3: Retry-based ingest (block per-ledger until WASM is known)

**Description:** Defer the persist decision on `Other`/NULL contracts
by buffering the ledger's NFT rows in memory and retrying later
(after the WASM upload arrives, somehow).

**Pros:**

- No new tables, no new pending state in the DB.

**Cons:**

- Per-ledger latency now depends on the wall-clock distance between
  a contract's first reference and its eventual `wasm_upload` op —
  unbounded in the general case (a contract can be observed forever
  without its WASM ever being reachable in the backfill window).
- Buffer-state on the indexer side is non-trivial and would need
  durable persistence anyway (Lambda cold starts, worker churn) —
  i.e. a pending table by another name.

**Decision:** REJECTED — solves the "in DB" problem by moving it to
"in indexer process memory"; no net win.

### Alternative 4: Parser-only filter (Patch C — payload-type whitelist)

**Description:** Discriminate NFT vs fungible at the parser layer by
restricting `looks_like_token_id` to a whitelist of conventional
`token_id` types (`u32`, `u64`, `i64`, `i32`, `bytes`, `string`,
`address`) and explicitly rejecting `i128` / `u128` (SEP-41 amount
shape). Shipped in PR #178 (task 0118 Patch C) on the assumption
that legitimate NFT contracts always use unsigned-integer token_ids
per SEP-50 + the OpenZeppelin Stellar `NonFungibleToken` trait.

**Pros at design time:**

- Cuts the bulk of false positives at the source (audit-measured
  XLM SAC volume drops from 421k rows to zero).
- Smallest change conceptually.

**Cons surfaced empirically:**

- **Bug #3 deeper finding (pre-audit re-test, 2026-05-13)** — Karol
  fetched a live mainnet NFT, James Bachini's SEP-39 collection
  `CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY`, via
  `stellar contract fetch`. The contract is a real, fully-functional
  NFT (`name = "SorobanNFT"`, `symbol = "SBN"`, exports `owner_of`,
  `token_uri`, `token_image`), and **`owner_of(token_id: i128)`**
  — i.e. its `token_id` is `i128`. SEP-39 (the older Stellar NFT
  spec / ERC-721-style) explicitly permitted `i128` token_ids;
  SEP-50's "unsigned integer" requirement is the newer convention.
  Both shapes coexist on mainnet.

- The 2026-05-12 CH pilot audit sample did **not** contain any
  SEP-39 NFT — its 15.7k-ledger window was biased to a single
  protocol-25 slice that happened to have no Bachini-style
  collections. Patch C's design rested on that sample. The
  pre-audit re-test against live RPC revealed the false-negative.

- Crucially: Patch C operates **before** the persist-time
  classifier can see the row. The WASM-spec-based classifier
  (`classify_contract_from_wasm_spec`, the architecturally correct
  discrimination point) is bypassed entirely for `i128`-shaped
  payloads. No amount of downstream sophistication (quarantine,
  reclassify hook, runbook drains) can recover rows that never
  reached the parser's emit.

**Decision:** REJECTED. Audit team's stated principle —
"discrimination MUSI być po WASM signature, NIE po payload type" —
is correct, and Patch C contradicts it. Patch C was reverted in the
same branch as PR #180 (this ADR's implementation); the parser
returned to its pre-Patch-C permissive blacklist
(`!void|map|vec|error`), and the test
`parser_emits_i128_transfer_as_nft_candidate` was restored with a
docstring referencing the SEP-39 mainnet example.

The 99.4% noise-reduction target that motivated Patch C is still
met by the quarantine pattern itself: `Fungible`/`Token`-classified
contracts drop at the persist filter (where the classifier verdict
is authoritative), and `Other`/NULL go to `_pending` (invisible to
the API). Real SEP-39 NFTs land in `_pending` initially and are
promoted to hot once their WASM upload is observed.

---

## Consequences

### Positive

- **API surface is clean by design.** `/v1/nfts*` queries the hot
  tables and sees only `Nft`-classified rows. No more
  "production looks dirty until backfill catches up" failure mode.
- **Promotion is atomic with reclassify.** Readers never observe
  a contract that is "classified Nft but has no rows in `nfts`" or
  vice versa.
- **Reusable pattern.** Other classifier-pending data
  (`assets_pending`, `liquidity_pools_pending`, …) can follow the
  same template if and when needed.
- **Operational decoupling from backfill.** Hot-table cleanliness
  no longer depends on completing the Soroban-era backfill. The
  backfill still drives the eventual drain of pending, but that's a
  storage-reclaim concern, not a correctness concern.

### Negative

- **2× write-path INSERT blocks** in `upsert_nfts_and_ownership` (one
  per hot/pending bucket). Bounded by the per-bucket index vectors;
  empty buckets short-circuit, so the cost on a "no Other contracts
  in this ledger" load is negligible.
- **Extra SELECT on every reclassify** to identify which
  contract_ids got which new verdict (over-selects slightly —
  returns contracts whose verdict was already Nft/Fungible before
  this ledger — but the helpers no-op on those). Acceptable in
  exchange for not threading a "was changed" bit through the
  UPDATE.
- **One-shot migration required** on environments that already have
  legacy `Other`/NULL rows in their hot tables (initial migration
  in the operational runbook). Per-environment, one-time.
- **Storage footprint** doubles transiently during the backfill
  for the `Other` bucket (rows live in pending until WASM arrives,
  at which point they move or drop). Bounded by the steady-state
  size of unclassified contracts; expected to be small in a
  fully-indexed system.

---

## Operational Impact

Lifecycle is documented in the operator runbook at
[`docs/runbooks/0217_nfts_pending_migration_and_drain.md`](../../docs/runbooks/0217_nfts_pending_migration_and_drain.md):

1. **On the 0217 deploy** — run §Part 1 (initial migration) to move
   existing `Other`/NULL rows out of the hot tables into the
   quarantine. Idempotent. Required only once per environment.
2. **Throughout the backfill** — the persist-time promotion hook
   drains the quarantine as WASM uploads arrive. No operator action.
3. **After the full Soroban-era backfill completes** — run §Part 2
   (drain) to promote any stragglers under contracts now classified
   as `Nft` and TRUNCATE the pending tables. Idempotent. Once per
   environment.

Cross-runbook ordering: **0118 Phase 3 cleanup runs first** (drops
legacy `Fungible`/`Token` rows from hot), then **0217 Part 1 initial
migration**, then the persist hook drives the steady-state.

---

## Delivery Checklist

Per [ADR 0032](./0032_docs-architecture-evergreen-maintenance.md):

- [ ] `docs/architecture/technical-design-general-overview.md` updated — N/A (no top-level system topology change; quarantine is an internal persist-path detail).
- [x] `docs/architecture/database-schema/database-schema-overview.md` updated — §4.13.1 added (NFT Quarantine subsection with PG DDL, routing table, design notes, runbook link).
- [ ] `docs/architecture/backend/backend-overview.md` updated — N/A (no backend surface change; API endpoints still read the same hot tables).
- [ ] `docs/architecture/frontend/frontend-overview.md` updated — N/A (no frontend change; quarantine is transparent to the FE).
- [ ] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` updated — N/A — the indexing pipeline overview describes the high-level pipeline stages; the per-bucket persist routing is an internal `crates/indexer/src/handler/persist/write.rs` detail, not a pipeline-stage change.
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md` updated — N/A (no infrastructure change).
- [ ] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` updated — N/A (parser side returns to the pre-2026-05-12 permissive blacklist after the Patch C revert in this PR; no shape change to document beyond the test rename, which is a code-level concern).
- [x] CH-side counterpart updated in [`docs/architecture/database-schema/clickhouse-pilot.md`](../../docs/architecture/database-schema/clickhouse-pilot.md) §4c-bis (additional to the boilerplate list above).
- [x] This ADR is linked from each updated doc at the relevant section.

---

## References

- [PR #178 — 0118 Patch C parser whitelist + Phase 3 cleanup runbook](https://github.com/rumblefishdev/soroban-block-explorer/pull/178) (Patch C subsequently reverted in PR #180)
- [PR #180 — 0217 quarantine implementation + 0118 Patch C revert](https://github.com/rumblefishdev/soroban-block-explorer/pull/180)
- [`docs/audits/2026-05-12-ch-pilot-endpoint-audit.md`](../../docs/audits/2026-05-12-ch-pilot-endpoint-audit.md) — empirical motivation (§E15–E17, 99.4% false positives)
- SEP-0041 fungible token interface — defines `amount: i128` as the canonical fungible payload type.
- SEP-0050 NFT interface — defines `token_id` as an unsigned integer.
- OpenZeppelin Stellar `NonFungibleToken` trait — uses `u32` for every `token_id` parameter (de-facto reference implementation).
