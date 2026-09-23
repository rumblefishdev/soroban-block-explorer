---
prefix: S
title: Implementation log — writer (step 2) and readers (step 3)
status: mature
---

# S — Implementation log

## Step 2 — writer — writer (2026-09-23)

Done, uncommitted on `docs/0575-presence-tables-canonical-position`:

- **Refactor first (pure move, own commit):** the participant loop and the
  event-derived asset loop left `stage.rs` (3,380 → 3,360 lines, over the
  limit) for `persist/stage/presence.rs`; 158/158 unit tests unchanged.
- **Change:** `TransactionParticipantRow` / `OperationAssetAppearanceRow` carry
  `application_order: i16` instead of `transaction_id`; the three staging
  sites read `app_order_by_hash`. `init.sql`: both tables in the target shape
  with the trial's codecs; the `transactions` header no longer lists
  `transaction_participants` as an `id` consumer; the schema guard's allowlist
  lost both tables (6 left).
- **Tests:** column-order and staging assertions in `tests_cross.rs` compare
  the position with the staged transaction's; `smoke.rs` insert;
  `backfill-runner` fixtures (`bootstrap.rs`, `repair_tier1.rs`). The
  backfill-runner's production SQL reads only `account_id` /
  `ledger_sequence` of these tables — unaffected.
- **Verified:** `cargo test -p db-clickhouse -p backfill-runner` green against
  a throwaway ClickHouse 26.3 with `init.sql` applied (smoke, `persist_e2e`
  and the other gated e2e tests write through the new structs; the server
  shows `CODEC(Delta(8), ZSTD(1))` / `CODEC(T64, ZSTD(1))`); clippy
  `-D warnings` clean.
- Trivia: a stray half-comment (`Some(v) = reduced; None = touched but`, left
  from the removed `net_settled`) dropped from the op-asset push.

Not shippable alone: the API still reads `transaction_id` from both tables —
step 3 lands in the same PR.

## Step 3 — readers — readers (2026-09-23)

Done, uncommitted, same branch:

- **Account list** (`accounts/queries.rs`, `handlers.rs`): driver seeks
  `(ledger_sequence, application_order)`, the page fetches `transactions` by
  its full key, rows are emitted in position order; the cursor is
  `ChPosition`, a surrogate cursor answers 400 `invalid_cursor`.
  `operation_types` still come from `operations_appearances` by `t.id`, so the
  aggregate now runs after the page (in parallel with the source-account and
  balance-change reads — same round-trip depth as before). `AccountTxRow.id`
  was read only by the old cursor and is gone.
- **Asset list** (`assets/queries.rs`, `handlers.rs`): same move; `AssetTxRow`
  carries `application_order` instead of `id`. The second arm over the
  asset's contract is removed (README, Design Decisions 1): one seek, no
  union; `union_keyset_arms` and its test went with it, and the decode smoke
  now pages twice with a position cursor.
- **Transaction page participants** (`transactions/queries.rs`): looked up by
  `(ledger_sequence, application_order)`.
- `TxListCursor` doc: account and asset lists listed under `ChPosition`.
- `backfill-runner/tests/account_reconciliation.rs`: the fee filter takes the
  position straight from `transaction_participants` (the hop through
  `transactions.id` is gone).
- **Tests moved to the repo layout (task 0525):** `accounts/queries_tests.rs`
  → `accounts/queries/tests.rs`, `assets/queries_tests.rs` and
  `assets/queries_decode_smoke.rs` → `assets/queries/`. New
  `accounts/queries/decode_smoke.rs`: two pages both directions with a
  position cursor, the cursor row never repeats.
- **Verified:** 881 tests green across `api`, `db-clickhouse`,
  `backfill-runner`; the account and asset decode smokes ran against a local
  ClickHouse 26.3 with `init.sql` and seeded rows (both arms of the asset
  union, both directions, a second page by cursor). The pool and search
  smokes need production-like data and fail on the seed — unrelated.
  `api-types:generate`: no diff (the cursor is an opaque string).
- **Docs:** endpoint queries 03, 07, 10 (10 describes the single seek); `database-schema-overview.md` §4.5 (the Postgres DDL left
  from before ClickHouse replaced with the real one) and §4.5.1;
  `clickhouse-pilot.md`; `indexing-pipeline-overview.md`.
