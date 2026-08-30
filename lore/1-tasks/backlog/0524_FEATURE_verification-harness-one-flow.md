---
id: '0524'
title: 'Verification harness — one flow with the traits of ALL its scattered ancestors'
type: FEATURE
status: backlog
related_adr: ['0057', '0058']
related_tasks: ['0361', '0382', '0423', '0210', '0503', '0502', '0374']
tags:
  [backend, clickhouse, validation, data-quality, priority-high, effort-large]
links: []
history:
  - date: '2026-08-30'
    status: backlog
    who: karolkow
    note: >
      Consolidation umbrella (decision karolkow 2026-08-30, option E): one
      verification system instead of five scattered tasks. Supersedes 0361;
      absorbs the intent of 0382/0423/0210; 0503 becomes this system's
      recurring RUN. The dead audit-harness crate is deleted alongside —
      its full invariant catalog is transcribed below, so nothing is lost.
---

# Verification harness — one flow, traits of all ancestors

## Summary

ONE command that answers "is our data right?" three ways in one pass —
internal invariants, reconciliation against the network's own state, and a
windowed pipeline-diff — instead of five parked tasks and a dead crate.
The 0374 e2e session ran exactly this by hand and caught four shipping
bugs; this task codifies that method.

## Design (the one-flow requirement is LOAD-BEARING)

Extend the existing `snapshot-seed` machinery (backfill-runner) — whose
dry-run already IS a reconciliation harness — with two more passes under
the same command, one decode, one summary.txt an operator signs:

1. **Invariant pass** (ancestor: the audit-harness crate, catalog below) —
   CH-native rewrites, every read RMT-deduped (the 0423 seeded-duplicates
   regression rides here as its test bed).
2. **Network reconciliation** (ancestors: 0463 seed, 0503) — the existing
   verdict machinery over the 0502 decoder; 0503 = this system on a cron.
3. **Windowed pipeline-diff** (ancestor: the 0374 e2e method) — ingest a
   real ledger window into a scratch CH, bidirectional diff state↔events in
   the LEDGER's order (application_order, then event_index — event_index is
   per-transaction!), spot-check against live RPC at each entry's own
   lastModifiedLedgerSeq.

**Oracles: the four from the standing rule** — RPC simulation / checkpoint
snapshots / the protocol's own API / raw XDR. Horizon is OUT everywhere
(standing verdict 2026-08-17); 0382's Horizon half and 0210's Horizon
parity die with it — their intent (never-silently-miss, supply parity)
lands on the surviving oracles.

## What supersedes what

- **0361** (port the crate) — superseded HERE: porting sqlx SQL is a
  rewrite anyway, and a second tool violates the one-flow decision.
- **0382 / 0210** — absorbed (re-oracled); close on this landing.
- **0423** — absorbed as the invariant pass's dedup test bed.
- **0503** — stays: it is the recurring RUN of this system.
- crate `audit-harness` — DELETED (catalog transcribed below; binaries
  read retired Postgres + legacy Horizon, nothing else to keep).

## Transcribed invariant catalog (from the deleted crate, verbatim labels)

PG-era table names; map to the CH schema when implementing, and remember
every read must dedup the unmerged RMT (argMax / GROUP BY) or counts lie.

- **01_ledgers**
  - I1 — sequence contiguous within indexed range
  - I2 — hash UNIQUE
  - I3 — closed_at strictly monotonic by sequence
  - I4 — non-negative counts
- **02_transactions**
  - I1 — hash UNIQUE across partitions (uq_transactions_hash_created_at, but hash alone)
  - I2 — operation_count >= COUNT(operations_appearances rows) per tx
  - I3 — every transaction.ledger_sequence exists in ledgers
  - I4 — source_id FK valid (every source_id → accounts.id)
  - I5 — non-negative numeric fields
  - I6 — inner_tx_hash either NULL or 32 bytes (matches CHECK)
- **03_transaction_hash_index**
  - I1 — every hash routes to existing transactions row
  - I2 — every transactions row has matching hash_index entry
  - I3 — hash UNIQUE
  - I4 — hash exactly 32 bytes (matches CHECK)
- **04_operations_appearances**
  - I1 — every (transaction_id, created_at) → existing transactions row (composite FK)
  - I2 — source_id FK valid where set
  - I3 — destination_id FK valid where set
  - I4 — asset_issuer_id FK valid where set
  - I5 — pool_id FK valid where set
  - I6 — amount (folded duplicate count) >= 1 when present
- **05_transaction_participants**
  - I1 — composite FK to transactions valid
  - I2 — account_id FK to accounts valid
  - I3 — composite UNIQUE (transaction_id, account_id, created_at) — no duplicate participation
- **06_soroban_contracts**
  - I1 — contract_id matches StrKey shape (56 chars, prefix C, base32)
  - I2 — contract_id UNIQUE
  - I3 — deployer_id FK valid where set
  - I4 — wasm_hash (when set) → wasm_interface_metadata.wasm_hash
  - I5 — contract_type SMALLINT in known range (per ADR 0031 + ADR 0036)
  - I6 — wasm_hash exactly 32 bytes when set
- **07_wasm_interface_metadata**
  - I1 — wasm_hash UNIQUE (PK)
  - I2 — wasm_hash exactly 32 bytes
  - I3 — metadata is valid JSONB object (not NULL, not array, not scalar)
- **08_soroban_events_appearances**
  - I1 — composite FK to transactions valid
  - I2 — contract_id FK to soroban_contracts valid
  - I3 — ledger_sequence matches the parent transaction.ledger_sequence
  - I4 — amount (folded duplicates) >= 1 when present
- **09_soroban_invocations_appearances**
  - I1 — composite FK to transactions valid
  - I2 — contract_id FK to soroban_contracts valid
  - I3 — caller_id FK to accounts valid where set
  - I4 — ledger_sequence matches parent transaction.ledger_sequence
  - I5 — amount (folded duplicates) >= 1 when present
  - I6 — every invoked contract has at least one event appearance OR is a no-event invocation
- **10_assets**
  - I1 — asset_type SMALLINT in known range (0-3 per ADR 0036)
  - I2 — ck_assets_identity per ADR 0038 (validate shape per type)
  - I3 — uidx_assets_native singleton (exactly one row with asset_type=0)
  - I4 — issuer_id FK valid where set
  - I5 — contract_id FK to soroban_contracts valid where set
  - I6 — non-negative supply / holder count
- **11_accounts**
  - I1 — account_id matches StrKey shape (G or M prefix, 56 or 69 chars, base32)
  - I2 — account_id UNIQUE
  - I3 — first_seen_ledger ≤ last_seen_ledger (monotonic)
  - I4 — non-negative ledger sequences
  - I5 — every account that is the source of ≥1 transaction in the dataset has sequence_number > 0
- **12_account_balances_current**
  - I1 — account_id FK valid
  - I2 — issuer_id FK valid where set
  - I3 — asset_type=0 (native) row has NULL asset_code/issuer_id; non-native has both
  - I4 — balance ≥ 0 (NUMERIC stored as NUMERIC(28,7))
  - I5 — uidx_abc_native: at most one native row per account_id
  - I6 — uidx_abc_credit: (account_id, asset_code, issuer_id) UNIQUE for non-native
- **13_nfts**
  - I1 — (contract_id, token_id) UNIQUE
  - I2 — contract_id FK to soroban_contracts valid
  - I3 — current_owner_id FK to accounts valid where set
  - I4 — minted_at_ledger ≤ current_owner_ledger (monotonic, mint precedes any transfer)
  - I5 — last nft_ownership row per nft → matches nfts.current_owner_id (mat-view consistency)
- **14_nft_ownership**
  - I1 — nft_id FK to nfts valid
  - I2 — transaction_id FK valid
  - I3 — owner_id FK to accounts valid where set
  - I4 — event_type SMALLINT in valid range (mint/transfer/burn enum)
  - I5 — first event per nft is a mint (event_type denoting mint)
  - I6 — event_order non-negative within ledger
- **15_liquidity_pools**
  - I1 — pool_id is 32 bytes (SHA-256 of asset pair per Stellar protocol)
  - I2 — pool_id UNIQUE (PK)
  - I3 — asset_a < asset_b type/code ordering enforced (Stellar canonicalises pair order)
  - I4 — issuer FK valid where set (asset_a, asset_b)
  - I5 — fee_bps in [0, 10000] (basis points)
  - I6 — sentinel placeholder pool count (informational, not a violation)
- **16_liquidity_pool_snapshots**
  - I1 — pool_id FK to liquidity_pools valid
  - I2 — non-negative reserves and shares
  - I3 — analytics fields (tvl, volume, fee_revenue) non-negative when set
  - I4 — at most one snapshot per (pool_id, ledger_sequence) — uq_lp_snapshots_pool_ledger
  - I5 — ledger_sequence corresponds to existing ledgers row
- **17_lp_positions**
  - I1 — pool_id FK valid
  - I2 — account_id FK valid
  - I3 — shares ≥ 0 (zero shares retained for future-history per task 0162 emerged decision)
  - I4 — first_deposit_ledger ≤ last_updated_ledger (monotonic)
  - I5 — (pool_id, account_id) UNIQUE (composite PK)
  - I6 — sum of active positions per pool ≈ latest snapshot.total_shares (within stale tolerance)
- **18_partition_routing**
  - I1 — count rows in \_default per parent (expect 0 across the board)
  - I2 — count children per parent (sanity: 30 monthly + 1 default = 31)
  - I3 — informational: rows-per-month heatmap (last 6 months of activity)

## Acceptance Criteria

- [ ] one command, three passes, one summary; no side subcommands
- [ ] invariant pass green on prod (or violations filed as bugs)
- [ ] reconciliation + pipeline-diff reproduce the 0374 session's checks
      unattended
- [ ] 0382/0210 closed as absorbed; 0423 covered by the dedup test bed
- [ ] 0503 wired as the recurring run
