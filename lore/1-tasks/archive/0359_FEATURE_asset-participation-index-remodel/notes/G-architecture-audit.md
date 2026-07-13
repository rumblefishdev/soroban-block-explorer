---
title: 'Architecture audit — patterns/anti-patterns catalog + strangler refactor plan (R1-R3, adoptions #1-3)'
type: generation
status: mature
spawned_from: '0359'
spawns: []
tags: [architecture, anti-patterns, indexer-canon, refactor-plan, handoff]
links:
  - https://claude.ai/code/artifact/0d4868da-8733-4258-bd0c-693696533061
history:
  - date: 2026-07-08
    status: mature
    who: karolkow
    note: >
      Serialized from two adversarial panels (4-lens code review; 5-agent
      indexer-canon hunt incl. web reference research) + /graphify knowledge
      graph (1333 nodes; graphify-out/graph.html). Every claim code-verified
      by a judge pass. Full visual version on the flow-map artifact.
---

# Architecture audit — full serialized catalog

## Judge verdict (indexer-canon panel)

This codebase is closer to canonical indexer architecture than most hand-rolled indexers: it already has the four hardest properties — a single live/backfill code path over shared parse+persist crates (Horizon's reingestHistoryRange equivalent), extract-once S3 raw-XDR archive (Firehose flat-files equivalent), idempotent replay via commit-marker-last writes + RMT dedup + refreshable-MV aggregates (delete-then-insert equivalent), and exhaustive no-panic value-layer parsing. The distance from canon is concentrated in three meta-layer disciplines, all VERIFIED in code: (1) protocol-upgrade handling is smeared across six TransactionMeta wildcard fallbacks that would silently absorb a Protocol-24 V5 meta — the one CRITICAL, the exact failure Horizon's version-stamped rebuild exists to prevent; (2) zero parser/ingestion version provenance on any row or run, so every emit-logic change (including the in-flight 0359 remodel) makes replayed ranges nondeterministic under versionless RMT and forces full archive re-parses as the only healing tool (already paid twice: 0261, 0359); (3) read-path correctness invariants (commit fence, arm limits, filter-before-limit) live in per-file convention, and the brand-new 0359 asset arms shipping WITHOUT the fence proves convention does not survive new code. Three highest-leverage adoptions: [1] a central meta-accessor module in xdr-parser (V0|V1|V2 explicit legacy arms, V3|V4 real arms, NO wildcard, plus UnsupportedMetaVersion error + a compile canary over OperationBody/OperationResultTr) — one file to touch on Protocol 24, kills the CRITICAL and the sibling-wildcard class; [2] provenance: an ingest_runs audit table (range, writer, git_sha, timestamps) now + LowCardinality parser_version column on fact tables at the 0359 re-parse (the re-parse is already being paid — stamping is nearly free and turns all future healing into targeted version-mismatch rewrites); [3] a shared driver-arm builder in common/ch.rs that appends the max(sequence) fence and enforces the two merge_tx_keys invariants by construction, fixing the 0359 fence gap and the Statement-B filter-after-limit defect class in one refactor.

## The 28-item catalog (verified)

### [ANTI-PATTERN | CRITICAL] Silent protocol-version absorption at the TransactionMeta layer (single upgrade point missing)

VERIFIED. Six independently-written V3/V4 dispatch sites each end in a wildcard: crates/xdr-parser/src/event.rs:116 (_ => Vec::new(), ALL events dropped), ledger_entry_changes.rs:100 (_ => {}, all entry changes dropped), contract.rs:55 (_ => {}), invocation.rs:439 (_ => Vec::new()) and :553 (_ => None), operation.rs op_meta_changes (_ => &[]) and soroban*return_value (* => None). A stellar-xdr TransactionMeta::V5 (Protocol 24) compiles clean at all six, producing zero events/changes/invocations/participations while transactions rows still persist and the ledgers commit marker still lands — the gap looks like a quiet chain. header.ledger_version is extracted (ledger.rs:25) but never consulted. Canonical counterpart: Horizon stamps an ingestion DB version and force-rebuilds on mismatch; The Graph halts deterministically instead of silently continuing. (Merged duplicate: 'smeared protocol-version knowledge' — same evidence, same fix.)

**Fix/adopt:** Create xdr-parser/src/meta.rs with soroban_meta(), op_changes(idx), diagnostic_events(), tx_changes() — match V0|V1|V2 explicitly as the legacy-empty arm and V3|V4 as real arms, NO wildcard, so a new enum variant breaks compile in exactly one file. Add ParseErrorKind::UnsupportedMetaVersion and fail the ledger loudly so the commit marker is never written for unparsed meta. Highest-leverage adoption #1.

### [ANTI-PATTERN | MAJOR] Missing max(sequence) read fence in 0359 asset-transactions arms (in-flight code, fixable now)

VERIFIED. Zero occurrences of the commit-marker fence in crates/api/src/assets/queries.rs; arm A (operation_asset_appearances seek, ~line 627) and arm B (soroban_invocations_appearances, ~line 648) seek unfenced, while exact analogues carry it (accounts/queries.rs:507, contracts/queries.rs:719, :735). Writer streams 18 tables and writes ledgers LAST (persist/writer.rs:8-12), so head keys are visible pre-commit; fetch_tx_page's INNER JOIN ledgers (assets/queries.rs:701) + by_id.remove-else-continue (:725) silently drop them → page shorter than limit+1 → finalize_page emits no next_cursor → the asset's entire older history truncates at the live head. Proof that the fence-by-convention does not survive new code.

**Fix/adopt:** Add the fence clause to both arms now (same as accounts/contracts). Then extract a shared fence fragment / driver-SQL builder in common/ch.rs next to merge_tx_keys and migrate the ~8 call sites so every future arm gets it by construction. Highest-leverage adoption #3.

### [ANTI-PATTERN | MAJOR] Partition-confined filtered global lists (silent truncation / false-empty at 500k boundary)

VERIFIED. All /v1/transactions statements pin each page to one intDiv(ledger_sequence,500000) partition: crates/api/src/transactions/queries.rs:464-465 (Statement B), :571-579 (C), :655-656 (A); module doc :29-32 accepts early-stop for backward pagination, but for FILTERED lists the first page is already wrong: a contract/op_type with no activity in the head partition returns an empty page with next_cursor=None (common/pagination.rs finalize_page: no excess → no cursor) although matches exist in older partitions. A sparse contract's whole history is unreachable via filter[contract_id]. Per-entity endpoints seek cross-partition correctly, so the gap is only the filtered global list.

**Fix/adopt:** Replace the single-partition pin with a descending partition-walk loop for filtered statements (query P, refill from P-1 until limit+1 keys or floor), or route filter[contract_id] to the multi-partition driver the /contracts/:id/transactions endpoint already uses.

### [ANTI-PATTERN | MAJOR] Overscan-then-filter without refill (false end-of-list on combined filters)

VERIFIED. Statement B truncates its driver at lim_over = params.limit \* 4 (transactions/queries.rs:423, LIMIT {lim_over} at :476/:583) and applies source_account/op_type predicates only in the outer query (:522-530). If fewer than limit+1 overscanned keys survive the post-filter (e.g. contract + source where the source signs few of the contract's txs), the page is short and next_cursor is omitted — pagination silently terminates with matching rows deeper. The 4x factor is an unvalidated magic constant with no refill loop.

**Fix/adopt:** Push filters into the driver arms (source can drive off transaction_participants intersected with contract arms) or wrap the driver in a refill loop advancing past the last examined key until limit+1 post-filter survivors or the partition/fence floor. At minimum distinguish 'filter ate the overscan' from 'exhausted' before omitting next_cursor. Codify as a merge_tx_keys invariant: no arm may filter AFTER its own LIMIT.

### [ANTI-PATTERN | MAJOR] Sibling wildcards behind one exhaustive gate (new op type ships with zero participations)

VERIFIED. extract*op_details is exhaustive over OperationBody (operation.rs:303-534, compile-forces a decision on new op types), but sibling extractors over the same op wildcard away: emit_asset_participations * => {} (participations.rs:135 — comment says 'recorded N/A', nothing records), extract*counterparties * => {} (operation.rs ~:223), claim*atoms * => &[] over OperationResultTr (operation.rs:126 — a future atom-bearing result, exactly how CAP-38 added atoms, would be silently missed), ledger*key_owner * => None. A protocol op added tomorrow compiles only after its details arm exists, yet ships with zero participations/counterparties — precisely the gap class task 0359 is remediating for offers.

**Fix/adopt:** Add a compile canary: a #[allow(dead_code)] fn matching OperationBody and OperationResultTr exhaustively with unit arms and a comment listing the extractors to revisit — any new variant breaks the build there. Include it in the central meta/canary module from the CRITICAL fix.

### [ANTI-PATTERN | MAJOR] No parser/ingestion version provenance (nondeterministic RMT replays, unhealable ranges)

VERIFIED. grep for parser_version/git_sha/build_info across crates finds only HTTP user-agent and OpenAPI version strings; no table in db-clickhouse/schema/init.sql and no row struct carries a code-version stamp; no ingest_runs audit table exists. Most fact tables are VERSIONLESS ReplacingMergeTree (operations_appearances, transactions, transaction_participants, operation_asset_appearances, soroban_events, assets) — when emit logic changes (as 0359 changes it right now) and a range is replayed, old-code and new-code rows for the same key merge arbitrarily, and nothing records which code wrote which ledgers. Canonical counterparts: Horizon's ingestion DB version stamp forcing rebuild on mismatch; Substreams' versioned module outputs invalidating caches. Consequence already realized: every 'is this range stale?' question is answered with a full 11M-ledger archive re-parse (0261, 0359).

**Fix/adopt:** Now: ingest_runs audit table (ledger_from, ledger_to, writer='live|backfill', git_sha + CARGO_PKG_VERSION, started/finished) written once per Lambda batch / backfill partition — zero hot-path cost. At the 0359 re-parse (already paid for): add LowCardinality(UInt16) parser_version to high-churn fact tables, bump a const in xdr-parser on any emit change, use it as RMT version tiebreaker so replays are deterministic and healing becomes targeted version-mismatch rewrites. Highest-leverage adoption #2.

### [ANTI-PATTERN | MAJOR] Fold-at-ingest grain loss — the recurring full-re-parse tax

VERIFIED. The 0163 OpKey fold (db-clickhouse/src/persist/stage.rs:930-1019) collapses same-key ops per tx into one row with amount: agg.count (a column named 'amount' holding a fold COUNT) and only min_apply_order kept — individual operation_index values, per-op monetary amounts, and per-op ordering are unrecoverable without an S3 archive re-parse. This cost has materialized twice (0261 pool_ids retrofit; 0359 exists because the single-asset-slot fold lost offers/path-legs/native). The new operation_asset_appearances repeats the bet at finer grain: identical (asset,role) legs in one op collapse under the RMT key (init.sql 'deliberately NO leg ordinal'), so per-asset crossing COUNTS are already unrecoverable for the next feature. Related doc contradiction: stage.rs:117-120 claims 'leg_index already disambiguates' but Participation has no leg_index and both participations.rs:33-35 and the schema state duplicates deliberately collapse.

**Fix/adopt:** Adopt a checklist item in the task template: every fold ships with a written 'what is unrecoverable' ledger. For 0359 specifically — the full re-parse is already being paid, so persist the one cheap extra grain now (leg ordinal or per-(asset,role) count column); regrowing it later costs another full S3 era re-parse. Fix the stage.rs:118-120 comment to state the actual collapse contract.

### [ANTI-PATTERN | MAJOR] No poison-pill quarantine — a permanently-failing ledger stalls the live tail forever

VERIFIED. Live cursor = max(sequence)+1 from ledgers (indexer/src/handler/mod.rs:200-216); is_transient_ch_error (mod.rs:579-588) only matches SchemaError::Query variants, so SchemaError::Staging is never retryable and permanent CH codes fail loud; there is no skip/quarantine/park mechanism, so every subsequent doorbell retries the same ledger while doorbells drain to the DLQ and the cursor never advances. Staging errors are reachable from data-dependent overflows (application_order/event_index/operation_index > i16, decimal parse in stage.rs). Mostly parser-bug-triggered — but one parser bug on one ledger = indefinite tail stall, and an operator cannot distinguish 'CH down' from 'poison ledger' without log archaeology. Fail-loud is arguably intentional (halt beats silent gap), which caps this below the CRITICAL.

**Fix/adopt:** On permanent persist failure after retries, write a row to an ingest_quarantine table (ledger_sequence, error_class, first_seen, attempts) + a dedicated CW metric/alarm; keep halt-by-default but make the poison ledger identifiable in one query, and optionally support an operator-set skip marker the reconcile consults. Document the stall-by-design + recovery runbook in docs/architecture.

### [ANTI-PATTERN | MAJOR] Three divergent asset-code normalizations (identity fracture + '<invalid>' collision)

VERIFIED. The same chain-controlled 4/12-byte code field is normalized three incompatible ways: (1) strict from_utf8 with literal '<invalid>' fallback + trim trailing NULs — operation.rs:584-629 (format_asset/format_change_trust_asset/format_asset_code) and participations.rs:235-240 (feeds the asset_id surrogate hash); (2) from_utf8_lossy (U+FFFD) + trim trailing NULs — ledger_entry_changes.rs:354/359/400/405; (3) cut-at-first-NUL then lossy — sac.rs:252-257 asset_code_to_string. A hostile-but-legal non-UTF8 code becomes '<invalid>' in participation-derived asset_ids but '\u{FFFD}…' in trustline/SAC identities — the SAME asset splits into different asset_ids across pipelines — and ALL distinct invalid codes from one issuer collapse onto '<invalid>:ISSUER' (silent cross-asset aggregation). Rare on mainnet (low likelihood) but permanent identity corruption when it occurs, hence ranked last of the MAJORs.

**Fix/adopt:** One shared asset_code_str() used by all four sites with a byte-exact injective policy (valid UTF-8 up to first NUL, else hex-escape raw bytes — never a shared sentinel). Do it inside the 0359 re-parse window since participation surrogate ids change for affected codes; note in the backfill plan.

### [ANTI-PATTERN | MINOR] ledgers RMT duplicates leak to the wire (no FINAL / LIMIT 1 BY on /v1/ledgers)

ACCEPTED (spot-checked schema comment context; hunter line refs consistent). init.sql:89-97 documents that parallel-backfill overlap produced duplicate ledgers.sequence rows needing a manual OPTIMIZE DEDUPLICATE (task 0228 incident), yet the /v1/ledgers list reads with no FINAL/LIMIT 1 BY/Rust dedup (api/src/ledgers/queries.rs:204-227), and the ledger-detail embedded tx list INNER JOINs ledgers without LIMIT 1 BY (queries.rs:271-291) — a duplicated ledger row renders twice and fans embedded txs x2 against LIMIT. Every other endpoint has an explicit dedup discipline; ledgers is the gap.

**Fix/adopt:** Add LIMIT 1 BY l.sequence to the ledgers list statement and LIMIT 1 BY t.application_order (or a deduped derived table) to the embedded tx query — near-zero cost on a unique monotonic key.

### [ANTI-PATTERN | MINOR] Cursor not bound to its filter set / statement (tiebreak semantics fork)

ACCEPTED. /transactions cursor tiebreak means application_order under Statement A but transactions.id (cityhash64 i64) under B/C (transactions/handlers.rs:214-222); nothing binds the cursor to its filter set, so a client that keeps a cursor while changing filters gets the other statement comparing (ledger, ordinal) against (ledger, hash) — silent dups/drops within the boundary ledger, contradicting the codebase's own strict-cursor philosophy (common/cursor.rs rejects every other malformed case loudly).

**Fix/adopt:** Add a statement/filter fingerprint field to the cursor and reject mismatch with the existing invalid_cursor 400 — consistent with the envelope's deny_unknown_fields strictness.

### [ANTI-PATTERN | MINOR] Dead read-path weight: never-read 1M-cell dictionary, no-op bloom index, dead columns

ACCEPTED. transaction_hash_dict (init.sql:738-754, COMPLEX_KEY_CACHE, LIFETIME 300-360 → reloaded every ~5min forever) has zero production readers — the hash hot path reads transaction_hash_index directly; idx_tx_hash_bloom on transactions cannot prune because every hash-filtered read also carries the PK-prefix ledger_sequence equality; soroban_events.signature has no API reader; assets.total_supply/holder_count/icon_url are documented-dead awaiting ALTERs (0304/0310).

**Fix/adopt:** Drop the dictionary (or actually wire it as the O(1) hash hot path — pick one), DROP INDEX idx_tx_hash_bloom after EXPLAIN confirms zero pruning, batch the dead-column ALTERs with the pending 0304/0310 drops.

### [ANTI-PATTERN | MINOR] HashMap iteration order reaches emitted output (violates the parser's own determinism contract)

ACCEPTED. state.rs extract_account_states and nft.rs detect_undeployed_sac_overrides collect from HashMap in iteration order; keys are unique so RMT semantics survive, but types.rs:534-540 promises 'identical between live ingest and archive backfill re-parse', and nondeterministic row order makes live-vs-backfill byte diffing (the state-verification pattern) noisy.

**Fix/adopt:** Sort by key before returning (or BTreeMap) — two one-line changes, no schema impact.

### [ANTI-PATTERN | MINOR] Forward-compat encoding losses in details JSON: muxed-id dropped, u256/i256 as raw hex

ACCEPTED (merged two parser MINORs). (a) muxed_to_g_strkey (envelope.rs:36-42) drops the 8-byte mux id per ADR 0026, but the canonicalization also reaches details JSON destinations, so M-address routing is unrecoverable from any column post-P23 (CAP-67 made muxed first-class); event_filters.rs map_amount walks the exact data map holding to_muxed_id and discards it. (b) scval_to_typed_json emits u128/i128 as decimal strings but u256/i256 as undelimited two's-complement hex (scval.rs:31-44), so numeric_scval refuses 256-bit amounts and a u256-amount SEP-41 token silently gets amount=None. Both conflict with the fundamental-complete-backward-data principle: each costs a full re-parse later.

**Fix/adopt:** Thread destinationMuxedId into op details JSON and a muxed_id field from parse_token_event (keep G-canonical keys); emit decimal strings for 256-bit values (big-int format from the 4 u64 limbs). Cheap now, re-parse-expensive later — good candidates to ride the 0359 re-parse.

### [PATTERN | PARTIAL] Total parsing / monitored-UNKNOWN discipline (canonical: Graph deterministic-halt, Substreams never-miss)

Strong in spots, VERIFIED both ways. HAVE: NFT pending-table quarantine + drain (stage.rs route_for, never-cache-Other), parse_error=true dead-letter flag persisted per tx with raw XDR on S3, aggregated missing-envelope warns, strict reject-not-partial SAC decode. VIOLATED: the two existing drop counters (diagnostic_dropped/contract_orphan_dropped, stage.rs:1022-1064; non-G key drops :581-591) emit only tracing::debug! — invisible at prod info level, never a CW metric — and multiple chain-data drop sites have no counter at all (pool-asset split failure stage.rs:814-819, unparseable balance asset_type :1509-1516, malformed claimedAtoms hex, cb/lp meta-miss in participations.rs:113-131, C-address counterparty filter :483-486). Nothing monitors the parse_error backlog either.

**Fix/adopt:** Promote counters to a per-batch CW metric next to LastProcessedLedgerSequence; thread a per-ledger DropStats through prepare_with_sac_overrides so every continue-on-data site increments; add a scheduled count on transactions WHERE parse_error=true. House rule: a continue/None discarding chain data must be provably impossible (commented invariant) or counted.

### [PATTERN | HAVE] Idempotent replay-safe writes (canonical: delete-range-then-insert / atomic cursor commit)

VERIFIED. Every non-ledgers insert completes before the ledgers commit-marker opens (persist/writer.rs:303-355); mid-crash leaves no ledgers row, resume re-does the range, orphans dedupe under RMT. The two non-RMT engines are replay-safe: asset_sac AggregatingMergeTree max-merges idempotently; balance_aggregates is filled only by a refreshable full-recompute MV with atomic EXCHANGE — replayed inserts can never double-count. Deterministic natural keys throughout. This matches stellar-etl's delete-then-insert and Substreams' cursor-with-data guarantees.

**Fix/adopt:** Keep. The residual gap is versionless-RMT nondeterminism under code change — closed by the provenance adoption (parser_version as RMT version tiebreaker).

### [PATTERN | PARTIAL] Commit-marker read fence (write ledgers last, read <= max(sequence))

VERIFIED design, convention-only enforcement. 'Ledger row visible => all its rows visible' holds by writer ordering; reads exploit it via explicit fences (transactions/accounts/contracts/liquidity_pools queries) and INNER JOIN ledgers on detail reads; ETag head inlined so body==validator. State-of-the-art for CH without snapshots. BUT the fence is re-written per query with no shared helper — and the 0359 asset arms shipped without it (the MAJOR above), proving convention does not scale to new code.

**Fix/adopt:** Extract the fence into a common/ch.rs fragment/builder and migrate the ~8 sites; review rule: every driver seek goes through the builder.

### [PATTERN | HAVE] Single live/backfill code path (canonical: Horizon reingestHistoryRange, Firehose one-format)

VERIFIED. backfill-runner/Cargo.toml links the same xdr-parser and db-clickhouse crates as the live indexer, and both backfill-runner/src/sink.rs and indexer/src/handler/process.rs call the same persist entrypoints (prepare/prepare_with_sac_overrides + writer) — the dedicated backfill binary is a source abstraction, not a second implementation, which is exactly the canonical exemption.

**Fix/adopt:** Keep. When adding the ingest_runs provenance table, record writer='live|backfill' so the shared path stays auditable.

### [PATTERN | PARTIAL] Extract-once raw archive + write-time decoding (canonical: Firehose flat files, Dune decoded tables)

VERIFIED shape. Raw ledger XDR persists on S3 and every re-model/backfill is a file re-parse, not a node replay (Firehose-equivalent); all list-serving facts are typed columns written at ingest; read-time XDR re-parse is correctly confined to the tx-detail endpoint per ADR 0029, spawn_blocking'd, degrading to heavy_fields_status=unavailable. PARTIAL because: zero caching by explicit admission (stellar_archive/mod.rs:10) — every detail request pays a cross-region S3 GET + ~1.5MB zstd + whole-ledger deserialize to extract ONE tx, and same-ledger bursts re-pay N times; plus a fully-built but caller-less fetch_ledgers/extract_e14_heavy path.

**Fix/adopt:** Add a small in-process LRU keyed by ledger sequence (parsed meta or raw bytes; Lambda memory fits a few ledgers), keep the 16-way concurrency cap; delete or feature-gate the unused batched path until an endpoint claims it.

### [PATTERN | PARTIAL] One processor per output table (canonical: Horizon ingest/processors)

VERIFIED structure. The single-pass-over-decoded-ledger property holds (one prepare_with_sac_overrides pass feeds ~20 tables — canonical), but the per-table separation does not: stage.rs is a 2,108-line monolith where all table populations interleave in one function family, with stale module-doc table counts (persist.rs '17 tables' vs writer.rs '18 streaming' vs 21 TableInserts slots). Adding a table means editing the shared pass, not registering a processor — the 0359 diff touching stage.rs/rows.rs/writer.rs simultaneously illustrates the coupling.

**Fix/adopt:** No rewrite needed — incrementally split stage.rs into per-table build\_\*\_rows modules invoked by the single pass (several already exist as helpers), and replace hardcoded table counts with 'see TableInserts'. Full Horizon-style processor registry only if table count keeps growing.

### [PATTERN | MISSING] State verification against an independent source (canonical: Horizon verifyRange)

No automated cross-check exists: nothing recomputes indexed state from an independent path (Horizon API, stellar.expert, or raw re-parse) on a schedule and alerts on divergence. A manual compare-with-stellar-api skill exists for ad-hoc verification, and the parser's determinism contract (types.rs:534-540) plus the S3 archive make automation cheap — but drift from processor bugs (e.g. the fold classes above) currently surfaces only when a human notices wrong numbers (as happened with F2/native and the 0359 offer gap).

**Fix/adopt:** Schedule a sampled verifier: pick N random accounts/assets per day, recompute balances/participation counts from Horizon or a raw re-parse of their ledgers, diff against CH, alarm on mismatch. Reuse the compare-with-stellar-api skill's logic as the seed.

### [PATTERN | HAVE] Exhaustive, panic-free value-layer parsing (canonical posture for hostile chain data)

VERIFIED. LedgerCloseMeta matched V0|V1|V2 with no wildcard (compile-breaks on V3); OperationBody all 27 variants; ScVal exhaustive incl. CAP-67 regression test; ClaimAtom incl. pre-P11 V0 (seen in code at operation.rs:227-239); LedgerEntryData/LedgerKey exhaustive. No reachable panic on hostile data: utf8 degrades, 64MiB zstd bomb cap, xdr depth/size limits, leb128 truncation bails, only-invariant expects. The gap is solely that the TransactionMeta layer does not follow the house style (the CRITICAL).

**Fix/adopt:** Keep; cite these files as house style in a parser README and extend the no-wildcard rule to TransactionMeta via the central accessor.

### [PATTERN | HAVE] Fail-open vs fail-closed per lookup, each with a named recovery path

VERIFIED in prior spot checks. The four write-time prefetches each document their failure mode and recovery: G1 wasm verdicts fail open → batch backstop; G9 contract verdicts fail open → quarantine + reclassify; 0320 prior rows fail open → wasm-upgrade-backfill; fetch_sac_classic_map fails CLOSED because a miss would silently commit orphaned balances (persist.rs:298-304). This is the correct generalization: fail-open only when a named batch process provably repairs the loss.

**Fix/adopt:** Write the rule down (persist module header or short ADR): every new prefetch/enrichment on the write path must declare fail-open+recovery or fail-closed. Costs a paragraph, prevents the next undocumented default.

### [PATTERN | HAVE] Denylist-permanent retry classifier with tested boundaries

VERIFIED. indexer/src/handler/mod.rs:579-676: retry-by-default with an explicit denylist of permanent failures, a reasoned exclusion of CH code 49, justification for denylist-over-allowlist (crate surfaces bodies verbatim), and unit tests covering code-boundary collisions. Plus gap-free ordering from the durable cursor rather than SQS delivery order.

**Fix/adopt:** Keep; reuse the same classifier in any future CH-writing worker (enrichment backfill crate) instead of re-deriving.

### [PATTERN | HAVE] Merged-arm keyset pagination (merge_tx_keys) — provably correct under interleaving

VERIFIED. Each arm is exhaustive for its stream, ordered under the same strict tuple comparator, fetched with LIMIT >= merged limit; sort+dedup+truncate of the union yields exactly the global page; strict comparators exclude the cursor tuple; asc/desc and next/prev symmetric. The 0359 arm A even dedups server-side with LIMIT 1 BY before the page limit (the fan-out fix). The proof rests on two unwritten invariants that Statement B already violates elsewhere.

**Fix/adopt:** Codify in merge_tx_keys docs + debug_assert: (1) every arm fetched with LIMIT >= merged limit, (2) no arm filters after its own LIMIT. A future arm violating either silently breaks the proof.

### [PATTERN | PARTIAL] SQL injection discipline (bind for strings, inline only provably-numeric)

VERIFIED clean surface: all user-controlled strings go through .bind(); format!-inlined values are i64/i16 or 'static fragments; user StrKeys are hashed to i64 surrogates before inlining; the inline-vs-bind split has a real driver rationale (clickhouse 0.15 NULL-in-tuple bug). PARTIAL because the invariant lives in ~40 repeated comments with zero type-level enforcement — one future String inline breaks it invisibly.

**Fix/adopt:** Micro-discipline, not a query builder: an InlineNum(i64) newtype with Display + an in_tuples helper (three sites already hand-roll it identically), so 'inlined = provably numeric' is compiler-enforced.

### [PATTERN | PARTIAL] Reorg/finality handling — legitimately N/A but undocumented

Stellar SCP gives instant finality, so the canonical hot-range/rollback machinery (Ponder/Subsquid/Substreams undo signals) is legitimately absent — the reference catalog itself notes instant-finality chains may skip it, 'but that must be a documented decision, not an omission'. No doc in docs/architecture or the persist/indexer module headers states the finality assumption.

**Fix/adopt:** One paragraph in docs/architecture (ingestion doc) where a reorg handler would otherwise live: 'SCP finality => no rollback path; a ledgers row is immutable once committed.' Zero code.

### [PATTERN | PARTIAL] Ingestion lifecycle state machine + version stamp (canonical: Horizon FSM)

The pieces exist informally: persisted watermark (max(sequence) cursor), single-writer-by-construction (Lambda + doorbell), commit-marker atomicity, budget-bounded reconcile loop. Missing relative to Horizon's FSM: no stored ingestion-version whose mismatch forces rebuild (see provenance MAJOR), no named quarantine/skip state (see poison-pill MAJOR), no explicit gap-heal state — gaps are healed by ad-hoc backfill invocations rather than a named lifecycle transition.

**Fix/adopt:** Do not build an FSM wholesale; the provenance (ingest_runs + version) and quarantine adoptions above close the two gaps that matter. Name the recovery procedures in a runbook so operations are states-on-paper even if not states-in-code.

## Strangler refactor plan (from the graph-based architecture analysis)

Knowledge graph: /graphify over 58 pipeline files → 1333 nodes / 2521 edges /
65 communities (`graphify-out/graph.html`). Confirmed: God-Payload details-JSON
contract (4 string-matchers vs typed side-channel = concept_dual_extraction;
claim atoms in 3 representations; asset identity in 3 encodings);
arms→merge→hydrate read pattern triplicated; dead `account_balances_current`.

- **R1 — shared tx-feed engine (api/common):** `tx_feed(arms: Vec<ArmSpec>) → Page`
  — kills page-assembly triplication; commit fence + merge invariants +
  filter-before-limit enforced BY CONSTRUCTION (the 0359 fence gap proved
  convention does not survive new code). No data changes. Smallest/first.
- **R2 — typed OpFacts IR in the parser:** parser emits typed facts; details-JSON
  becomes a DERIVED view (one render fn); staging drops all 4 string-matchers.
  Kills the God-Payload bug class (parent of the 0359 root cause). Pure code
  refactor — NO re-parse, NO table changes; can land after the backfill.
- **R3 — ops-phase cleanup:** drop dead `account_balances_current`, 0334 bloom
  `idx_oa_asset_issuer_id`, legacy asset columns on operations_appearances
  (post-backfill validation), never-read 1M-cell dictionary.

## Adoption priorities (canon distance)

1. **meta.rs central protocol accessor** (kills the CRITICAL silent-V5
   absorption; one file to touch on Protocol 24) + explicit variant listing in
   sibling emitters (emit_asset_participations / extract_counterparties /
   claim_atoms).
2. **Provenance:** `ingest_runs` audit table + `parser_version` LowCardinality
   column on fact tables — stamp during the 0359 backfill (nearly free then).
3. **R1 feed engine** (above) + automate the G-role-crossref contract as a
   verify-range job (the one MISSING canon pattern).
