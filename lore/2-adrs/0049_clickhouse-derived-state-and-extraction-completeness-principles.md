---
id: '0049'
title: 'ClickHouse derived-state & extraction-completeness principles (collapse at read, not at write)'
status: proposed # proposed | accepted | deprecated | superseded
deciders: [karolkow]
related_tasks: ['0283', '0231', '0243', '0221', '0218']
related_adrs: ['0044', '0046', '0048']
tags:
  [
    clickhouse,
    indexer,
    xdr-parsing,
    write-strategy,
    schema,
    architecture-principle,
  ]
links: []
history:
  - date: '2026-06-15'
    status: proposed
    who: karolkow
    note: >
      Created post-0283 investigation. Generalises a pattern found across
      ~7 distinct bugs (deploy-linkage, wasm-upgrade, interface-Restored,
      AccountMerge tombstone, Bachini i128, G5 name-clobber, SAC-skeleton
      exposure) plus the NFT-verdict fix into one principle, two families,
      two methods and the cross-ledger writer-prefetch pattern. Extends
      ADR 0048 (which solved only the
      two-independent-writers special case) to the general
      single-writer-multi-cadence and lossy-extraction cases.
---

# ADR 0049: ClickHouse derived-state & extraction-completeness principles

**Related:**

- [Task 0283: CH never writes Nft/Fungible verdicts](../1-tasks/active/0283_BUG_ch-contract-type-rebuild-from-wasm-metadata/README.md)
- [ADR 0048: Separate tables for two-writer columns](./0048_clickhouse-separate-tables-for-two-writer-columns.md) — the special case this generalises
- [ADR 0046: classifier / quarantine tables](./0046_classifier-quarantine-tables-nfts-pending.md)
- [ADR 0044: ClickHouse pilot — no FINAL / no mutations conventions](./0044_clickhouse-pilot-parallel-store.md)

---

## Context

The 0283 investigation surfaced ~7 distinct indexer/ClickHouse bugs that
looked unrelated but share **one root shape**. They were about to be fixed
one-by-one with bespoke solutions. A pattern hunt (0283 session 2) showed they
fall into **two families** with a **single underlying cause**, and that the
codebase _already_ solves instances of both ad-hoc — without a stated rule, so
new code keeps reintroducing the same class.

**The bugs (and where the codebase already handles the same shape correctly):**

| problem                                                                                                      | family         | already-correct precedent in repo                                            |
| ------------------------------------------------------------------------------------------------------------ | -------------- | ---------------------------------------------------------------------------- |
| deploy-linkage — `created`-only deploy filter drops `restored`                                               | A              | `extract_liquidity_pools` handles `created\|updated\|restored\|state` (0189) |
| wasm-upgrade — drops `updated` contract-instance entries                                                     | A + B          | `extract_lp_positions` handles `…\|removed`                                  |
| interface-`Restored` — `extract_contract_interfaces` drops `Restored` (also defeats the G1 verdict prefetch) | A              | `detect_classic_credit_assets` handles `removed` with key fallback           |
| AccountMerge — `extract_account_states` drops `removed` for accounts → stale balance, no zero tombstone      | A + B          | trustline `removed` path DOES emit a `balance=0` tombstone                   |
| Bachini / i128 — `token_id` events with an `i128` ScVal never extracted                                      | A (value-enum) | —                                                                            |
| G5 — name-only RMT row NULLs out wasm_hash/deployer/contract_type                                            | B              | `asset_enrichment` / `nft_enrichment` side tables (ADR 0048)                 |
| SAC-skeleton exposure — forward-derived routing-verdict rows pollute the public registry                     | B              | the verdict cache itself (0218/0221) is the materialised form                |

**Family A — lossy extraction.** A parser switches on a _closed enum_
(`change_type`, an ScVal type, an executable type) and processes only a subset,
silently dropping the rest via an implicit `_ => continue`. The dropped variants
carry real state (a `restored` entity, an `updated` mutation, a `removed`
deletion, an `i128`-typed value). The information is gone before it ever reaches
the DB.

**Family B — whole-row state mutation on ClickHouse.** An entity's state must
change over time, but the default engine `ReplacingMergeTree(version)` replaces
the **whole row** by highest version — there is no per-column merge and no cheap
`UPDATE` (ADR 0044/0048). So writing _part_ of an entity later either **clobbers**
the rest (G5, wasm-upgrade), **pollutes** the public table with speculative/
derived rows (SAC-skeleton), or forces the pure stage to **reconstruct
cross-ledger state it cannot see** (the NFT verdict fix).

ADR 0048 named Family B for the **two-independent-writers** case (indexer vs
enrichment worker) and prescribed side tables. It **correctly** distinguished
`soroban_contracts.name` — the ON-CHAIN soroban-token name (SEP-41 `Symbol("name")`
storage), a genuinely different value from a different source than the off-chain
`assets.name` / `nfts.name` it moved to side tables — and kept it inline as a
single-source, indexer-owned column. **That distinction is right and stands.**

The blind spot is narrower: even a _single on-chain writer_ writes that column at
**two cadences** — the deploy ledger and a later name ledger (late-init) — and on
CH whole-row RMT the late write clobbers the deploy fields just as a second writer
would. So the remedy for the single-writer case is **merge-discipline** (read the
prior row, re-emit complete), NOT necessarily a side table. ADR 0048 saw "two
writers → side table"; it did not cover "one writer, two cadences → merge".

---

## Decision

Adopt one meta-principle, formalise two failure families, and standardise two
methods (plus the cross-ledger writer-prefetch pattern) with an explicit decision
rule. Every new indexer/CH change is reviewed
against them; each 0283 open problem is fixed by its mapped method (not a bespoke
patch).

### Meta-principle

> **Collapse at read, not at write. Never reduce information prematurely.**

- Family A collapses the change-type/value enum _at parse time_ → loses state.
- Family B collapses multiple write-lifecycles _into one row_ → CH whole-row
  replace loses columns.

Both are premature reduction. The cure in both is the same: **preserve the full
structure; reduce only at the decision/read point.**

### Method 1 — exhaustive, documented enum handling (Family A)

Every `match`/`if` over a **closed on-chain enum** (`LedgerEntryChange` variant,
`ScVal` type, `ContractExecutable` type) MUST handle **every** variant with a
_deliberate, documented_ decision — handle it. The implicit `_ => continue` catch-all is banned on these enums.

- Mechanism: replace wildcard arms with an explicit arm per variant so the Rust
  compiler enforces exhaustiveness; a skipped variant gets a one-line comment
  (e.g. `state` = read-only observation snapshot, skipped to avoid double-count).
- This is **not** "process every variant" — some variants _should_ be skipped
  (handling `state` as a write double-counts). It is "make the skip a decision,
  not an accident."
- **Enforceability caveat (self-audit 2026-06-15).** The compiler-enforcement
  only works on NATIVE Rust enums (`LedgerEntryChange`, `ScVal`,
  `ContractExecutable`, `LedgerEntryData`) — and the codebase already complies
  there (`scval.rs`, `extract_single_change`, etc.). The real recurring gap is
  matches on the **`change_type` String** in `state.rs` (deploy `created`-only;
  account `removed` drop), where rustc CANNOT enforce exhaustiveness — a string
  `_ => continue` is invisible to the compiler. So "nearly free / compiler-
  enforced" is TRUE ONLY for the native-enum sites. To make Method 1 actually
  enforceable for the string sites, promote `change_type` to a typed enum (or a
  `#[non_exhaustive]` newtype with an exhaustive match); otherwise Method 1 is a
  code-review checklist there, not a compiler gate. Also: only apply to CLOSED
  on-chain enums — open/out-of-scope sets (event-name strings, host-function
  variants, config settings, pre-Soroban `TransactionMeta` versions) keep their
  wildcard correctly; forcing explicit arms there is noise.

### Method 2 — lifecycle-correct storage (Family B), as a decision rule

Columns of one logical entity that are written by **different writers** OR at
**different cadences** must NOT share an RMT row naively. Pick the storage shape
by _who writes and how_:

| situation                                                     | method                                                                                    | example                                               |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| two independent writers (cannot coordinate a read)            | **side table** + read-join/COALESCE (ADR 0048)                                            | `asset_enrichment`, `nft_enrichment`                  |
| one writer, multiple cadences (can read-before-write)         | **merge-discipline**: prefetch the prior row, re-emit a **complete** row, never a partial | **G5 name, wasm-upgrade**                             |
| pure aggregates                                               | `AggregatingMergeTree`                                                                    | holder_count/supply (batch today)                     |
| never-cleared + has a natural version + read-latency-critical | `CoalescingMergeTree` (per-case only)                                                     | (none yet)                                            |
| large historical / derived state                              | **batch reconciliation** from facts                                                       | repair-tier1, asset-aggregates, contract-type-rebuild |

**Default correction vs the naive reading of ADR 0048:** for _single-writer_
late/derived columns (name, current wasm_hash, verdict) the default is
**merge-discipline**, NOT a side table — it keeps the API read join-free and the
mutating writes are rare (~0.2 % of ledgers), so the extra prefetch is
negligible. Side tables are reserved for the genuinely two-writer case.

### The cross-ledger-context pattern — writer-prefetch (a first-class mechanism)

A distinct concern, NOT Family A (current-ledger drop) and NOT exactly Family B
(storage shape): **a per-ledger decision that needs PRIOR-ledger state.** The CH
stage is deliberately **pure — no DB reads** — for deterministic replay (an
ADR 0044-adjacent invariant). So any decision that depends on an earlier ledger's
already-stored value (e.g. "was this contract classified Nft in an earlier
ledger?", "what was the prior row before this late update?") is **blind** in the
stage.

**Pattern:** the **writer** (which holds the DB client) prefetches the needed
prior state into a map and passes it into the pure stage; the stage **composes**,
never reads. This keeps the stage pure and replayable while restoring
cross-ledger context.

This mechanism is **load-bearing in two roles**:

1. **Standalone need** — the NFT verdict fix: `fetch_prior_wasm_verdicts` (G1) and
   `fetch_prior_contract_verdicts` / routing cache (G9). Already shipped.
2. **The fix mechanism for Family B merge-discipline** — "read the prior row, then
   re-emit a complete row" IS a writer-prefetch (G5b, wasm-upgrade). So Family B's
   single-writer remedy is implemented _with_ this pattern.

It also underpins the SAC routing verdict (the skeleton rows are a _materialised_
form of this prior-state lookup) and balance/first-seen watermarks. Treat
writer-prefetch-into-map as the standard way to give a pure per-ledger stage the
prior-ledger context it needs — never add DB reads inside the stage.

**Out of scope:** ongoing differential verification against an external source
(Horizon / Soroban RPC / raw XDR via the `compare-with-stellar-api` tooling) would
_detect_ what Methods 1–2 miss, but it is runtime QA/monitoring, not a fix for the
bug class. Tracked separately, not by this ADR.

### Problem → method map

Verified by a 7-agent adversarial pass (0283 session 2): 6 confirmed, 1 refuted.

| problem                  | family                       | method to apply                                                                                                                                                          | verification                                                                                                                                 |
| ------------------------ | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| G5 name-clobber          | B                            | **version-0 guardrail SHIPPED** (name-only row can't outversion a deploy) + tripwire warn; NOT merge-discipline (moot — names off-ledger/empty, → enrichment side-table) | mechanism CONFIRMED, impact ZERO; root cause proven (names off-ledger)                                                                       |
| deploy-linkage           | A (create-extraction / meta) | investigate missed creates → re-parse / patch extractor; **NOT G5, NOT restored, NOT window**                                                                            | index genesis-complete → create WAS indexed but not extracted (meta-unavailable or unmatched deploy shape); orphans are Pass-2 stubs         |
| wasm-upgrade             | A + B                        | M1 (handle `updated`) + M2 merge-discipline; **low severity**                                                                                                            | CONFIRMED (low) — naive M1-only would clobber deployer                                                                                       |
| ~~interface-`Restored`~~ | —                            | **NOT NEEDED — reverted**                                                                                                                                                | operator confirms index is genesis-complete → `restored` duplicates an already-captured `created` (our store isn't archived); brings nothing |
| AccountMerge             | A + B                        | M1 (handle `removed`) + M2 zero-balance tombstone; **low-med**                                                                                                           | CONFIRMED (low-med) — partly accepted as 0228 skeleton floor                                                                                 |
| SAC-skeleton exposure    | B                            | M2 side-table — but read-filter is **safer for 0221**; decision point                                                                                                    | PARTIAL — correctness intact; verdict rows load-bearing for routing                                                                          |
| NFT verdict (done)       | B                            | M2 read-composition (prefetch + cache)                                                                                                                                   | shipped                                                                                                                                      |
| ~~Bachini i128~~         | —                            | —                                                                                                                                                                        | **REFUTED** — i128 not dropped; symptom of the classification gap, not extraction                                                            |

---

## Rationale

- **It is descriptive, not inventive.** The codebase already applies both methods
  correctly in places (LP state-handling for M1; enrichment side tables + repair
  jobs for M2). The ADR turns ad-hoc good practice into an enforced rule so the
  class stops recurring.
- **One frame replaces 7 bespoke fixes.** Reviewers and future sessions reason
  about _family + method_, not seven unrelated patches.
- **The decision rule prevents over-applying ADR 0048.** Naively "everything to a
  side table" would add a read-time JOIN to the API hot path for rare,
  single-writer columns. The rule routes those to the cheaper merge-discipline.
- **Compiler-enforced M1 is nearly free in Rust** — removing the wildcard turns a
  whole bug family into compile errors.

---

## Alternatives Considered

### Alternative 1: Event-sourcing / materialized-view projections

**Description:** Stop deciding at write time. Store raw facts (all changes, all
events); compute all derived state (classification, balances, verdicts) at read
time or via incremental MVs. The purest realisation of the meta-principle.

**Pros:** write path becomes dumb and nearly bug-proof; derived logic is
recomputable.

**Cons:** large re-architecture; MV correctness/cost at 9B-row scale unproven;
the project already half-does this (append-only fact tables + state tables) and
the incremental step (Methods 1–3) captures most of the benefit at a fraction of
the risk.

**Decision:** REJECTED for now — kept as the long-horizon direction; Methods 1–3
are the affordable down-payment.

### Alternative 2: Engine-per-need (don't force RMT everywhere)

**Description:** Some Family-B bugs come from using `ReplacingMergeTree`
(whole-row) where partial-merge or aggregation is wanted. Choose the engine per
table need: RMT for dedupe, `CoalescingMergeTree` for partial-merge state,
`AggregatingMergeTree` for aggregates.

**Pros:** fixes some clobber in-place, no read-join.

**Cons:** `CoalescingMergeTree` was measured and rejected globally in ADR 0048
(block-order non-determinism without a version, cannot clear to NULL, PROJECTIONs
blocked, risky core-table migration). Viable only per-case.

**Decision:** FOLDED INTO Method 2's decision rule as a per-case option, not a
blanket strategy.

### Alternative 3: Embrace batch reconciliation as the source of truth

**Description:** Accept that live writes are lossy/clobbering and rebuild all
derived state periodically from facts (the repair-tier1 pattern), rather than
fixing the write path.

**Pros:** cheap; already in use; robust for large derived state.

**Cons:** does **not** fix Family A — you cannot reconcile data you never
extracted; and a perpetual full-rebuild cadence is operationally heavier than
preventing the drop. Live correctness (routing, API freshness) still needs the
inline fix.

**Decision:** RETAINED as Method 2's option for large historical/derived state;
REJECTED as the _sole_ strategy.

### Alternative 4: Bespoke per-bug fixes (status quo)

**Description:** Fix each of the 7 independently.

**Cons:** no shared discipline → the class keeps returning in new code; reviewers
re-derive the same reasoning each time; ADR 0048's blind spot recurs.

**Decision:** REJECTED — the motivation for this ADR.

---

## Consequences

### Positive

- One reasoning frame for a recurring bug class; faster review.
- M1 is compiler-enforced → a whole family becomes impossible to merge.
- M2's rule keeps the API read path join-free for the common single-writer case.
- Generalises and completes ADR 0048 instead of leaving its blind spot latent.

### Negative

- M2 merge-discipline adds a prefetch read before rare mutations (acceptable;
  ~0.2 % of ledgers).
- Touching `extract_*` to remove wildcards is broad (many call sites) and each
  needs a deliberate per-variant decision + test.
- SAC-skeleton removal (M2) touches routing + must re-validate the 0221 SAC-leak
  guarantee — higher risk; sequenced as its own task.
- Existing 0283 live fix (G5) shifts from the earlier side-table lean to
  merge-discipline.

---

## Delivery Checklist

Per [ADR 0032](./0032_docs-architecture-evergreen-maintenance.md): this is a
principles ADR; the architecture-doc updates land with the per-problem
implementation tasks it spawns, not with the ADR itself.

- [ ] `docs/architecture/technical-design-general-overview.md` — N/A — principles ADR; no shape change yet (per-task)
- [ ] `docs/architecture/database-schema/database-schema-overview.md` — N/A — schema changes land with the G5/skeleton tasks
- [ ] `docs/architecture/backend/backend-overview.md` — N/A — no API change yet
- [ ] `docs/architecture/frontend/frontend-overview.md` — N/A — no frontend impact
- [ ] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` — N/A — per-task (M1/M2 changes)
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md` — N/A — no infra change
- [ ] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` — N/A — per-task (M1 extractor changes)
- [ ] This ADR is linked from each updated doc at the relevant section — pending the spawned tasks

---

## References

- ADR 0048 — the two-writer special case this generalises.
- Task 0283 session-2 notes — the pattern hunt and red/blue-team analysis.
- `compare-with-stellar-api` skill — Method 3 tooling.
