---
title: "Devil's-advocate pass — 7 adversarial agents vs every decision (core solid, packaging overreaches)"
type: synthesis
status: developing
spawned_from: notes/S-design-options.md
spawns: []
tags: ['red-team', 'devils-advocate', 'adr-input', 'scope', 'calibration']
links: []
history:
  - date: 2026-07-07
    status: developing
    who: karolkow
    note: >
      7 independent /devils-advocate agents each attacked one slice of the task,
      treating every project claim as interpretation and refuting against
      official Stellar sources (XDR .x, CAPs, Horizon, stellar-core, stellar-etl,
      ClickHouse docs). Verdicts: 6× "ship with changes", 1× "rethink packaging",
      0× "ship as-is", 0× "rethink core".
---

# Devil's-advocate pass — every decision stress-tested

## Method

Seven independent adversarial agents, one per slice (root-cause, fan-out-vs-
alternatives, native surrogate, backfill necessity, fat/thin, completeness
ceilings, scope/role/leg_index). Each: steel-man → pre-mortem/inversion/Socratic
→ verdict; each told to treat EVERY project claim (code comments, prod counts,
ADR statements, our own notes) as a possibly-wrong interpretation and to refute
against OFFICIAL external sources with URLs. Assumption baked in: the project may
be wrong at every step.

## Verdict: the CORE is solid, the DECISIONS overreach

No agent broke the diagnosis. Every agent weakened at least one decision or
justification. The re-model is likely the right end-state; it is **packaged as a
monolith, justified by claims softer than the notes present, and scheduled ahead
of the (thin) frontend that would consume it.**

## What SURVIVED (verified against canonical sources)

- Offers carry `selling`+`buying`; projection drops both → **HOLDS**
  (`Stellar-transaction.x` ManageSell/Buy/PassiveOfferOp).
- Path-payment carries `sendAsset`+`path<5>`+`destAsset`, only `destAsset` stored
  → **HOLDS**. The 7-asset ceiling (send + 5 hops + dest) → **HOLDS** (`path<5>`).
- Fixed-N slots die: result vectors `offers<>` / `offersClaimed<>` have **no XDR
  size bound** → **HOLDS**. (Nuance: practically bounded by the 1,000-op/ledger
  cap, not per-op — so "unbounded" is a type-level fact, not "infinite". Fixed-N
  still dies on the practical crossing count.)
- Backfill needs S3 re-parse: the dropped legs live in **no** ClickHouse column
  (no `details`/`envelope_xdr`/`result_meta_xdr` column exists) → **HOLDS**.
- Native surrogate is protocol-aligned (`ASSET_TYPE_NATIVE=0` is a first-class
  union arm; Horizon `asset_type:"native"`) → **HOLDS** mechanically.

## What was REFUTED / weakened — 13 challenged claims

| #   | Claim                                              | Status                                                                                                                                                                                                                                                                                                                                                      | Agent                  |
| --- | -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------- |
| 1   | "Single slot is THE root cause"                    | overclaim — 3 independent defects (slot / native-empty-string / read-side early-return)                                                                                                                                                                                                                                                                     | root-cause             |
| 2   | Native-first-class justifies fan-out over 2nd slot | **REFUTED** — the task's own red-team downgraded native to LOW/by-design; path-hops YAGNI'd; offers carried by both → native is motivated support                                                                                                                                                                                                           | native                 |
| 3   | "Unbounded ⇒ fan-out mandatory"                    | direction OK, but the FE is THIN → nothing renders the fan-out grain; crossings collapse to ≤1 tx-summary row/tx; fan-out is build-ahead-of-demand                                                                                                                                                                                                          | fan-out, scope         |
| 4   | "The only complete+correct+fast model"             | **REFUTED as absolute** — circular; completeness comes from the union regardless; fan-out's unique value is hot-key seek + role integrity (a handful of assets)                                                                                                                                                                                             | fan-out                |
| 5   | "Capture free now / else full re-backfill"         | **REFUTED** — CH `ADD COLUMN` is metadata-only; a later single-column mutation hardlinks untouched columns. Real tradeoff = one S3 pass now vs a narrower S3 pass later (amortization), not structural necessity                                                                                                                                            | fat/thin, backfill     |
| 6   | "6.4 B-op full backfill, cost accepted"            | a **bounded op-type-targeted** re-parse (only ledgers with types 2/3/4/12/13 + claimable) was never evaluated; payments already store their asset → potential large cut                                                                                                                                                                                     | backfill               |
| 7   | "Hops redundant with the trades stream"            | **REFUTED** unless the fan-out emits BOTH legs (`assetSold`+`assetBought`) of EVERY ClaimAtom; and declared `path[]` ≠ executed `offers<>`                                                                                                                                                                                                                  | completeness           |
| 8   | "Claim asset needs a CB-id → Create join"          | **REFUTED** — the claimed/clawed asset is in the SAME-OP `LedgerEntryChanges` (removed `ClaimableBalanceEntry`), which the parser already receives via `tx_meta`                                                                                                                                                                                            | backfill, completeness |
| 9   | `leg_index` bit-identical determinism              | **UNPROVEN** — the `pool_ids` analogy is false (set vs ordinal); same-asset-same-role atoms are the unhandled hard case; two crates must emit identical ordinals; HashMap iteration is nondeterministic                                                                                                                                                     | scope                  |
| 10  | Role enum is complete                              | **REFUTED** — misses/mis-buckets clawback, allow-trust/set-trustline-flags (third-party target), create-account starting balance, inflation; PoolShare 3-entity arity is open INSIDE a key column                                                                                                                                                           | scope                  |
| 11  | Keep everything in ONE epic                        | **REFUTED** — contradicts our own note (`R-audit-inventory` says Layer-2 "spawn its own epic"); one acceptance gate over ~20 findings + two 6–10 B-row backfills is unshippable                                                                                                                                                                             | scope                  |
| 12  | Native surrogate is "safe/correct"                 | **UNPROVEN as stated** — `asset_id` is a 64-bit truncated CityHash in ONE shared space (birthday bound); the magic constant `hash64("native")` must reproduce bit-identically across live + backfill (drift landmine); the stream-union re-introduces a THIRD native key (native-SAC surrogate) that must be OR'd or ~3.9 M XLM-SAC transfers silently drop | native                 |
| 13  | Loss numbers (37.5%, offers 1.37 B)                | **UNPROVEN externally** — quota-limited prod self-reports; Horizon can't count ops-by-asset; re-derive on FINAL (not raw RMT rows — the task already caught a 1.4× raw-vs-FINAL error on K2-2)                                                                                                                                                              | root-cause             |

## Convergent recommendation — SEQUENCE, don't build-ahead (4 agents independently)

Fan-out is likely the right **end-state**, premature at full width against a THIN
frontend. Ship by confirmed value:

- **Phase 0 — now, ZERO backfill.** Native positive surrogate + F-F SAC-union
  cheap win (`queries_ch.rs:222` already loads `sac_contract_surrogate`, unused).
  Closes the flagship symptom (native shows "No transactions") and surfaces
  native's ~3.9 M XLM-SAC transfers. Both already in Tier-0 of
  [S-field-comparison-fat-thin](S-field-comparison-fat-thin.md).
- **Phase 1 — narrow.** Offers indexed by asset (the one other confirmed HIGH,
  ~1.37 B). Evaluate whether a single offer-asset participation / offers-only
  fan-out suffices before the universal table.
- **Phase 2 — the full role-tagged fan-out + result-meta capture.** Gate on an
  actual **frontend render spec** that renders per-leg role / amount / trades
  (a per-asset Trades tab, "sent 49.65 USDC" rows). Today the FE renders none of
  it. Cost of the result-side capture is SMALL (the `OperationResult` is already
  deserialized live — `operation.rs:100-171`), so when Phase 2 runs, capture the
  trade grain in the same pass; the gate is DISPLAY readiness, not capture cost.

## Two CRITICAL gates before this becomes an ADR

1. **Split Layer-2 out.** `soroban_events` (9.5 B rows) token-flow decode is a
   co-equal epic per our own audit — give it its own task + acceptance gate.
   Move fee-bump / NFT / search / aggregate-hygiene to sibling tasks that
   reference 0359 but ship independently. Keep 0359 = Layer-1 classic
   participation + the F-F cheap win.
2. **`leg_index` = content-addressed + differential test.** Derive it from a
   stable hash of `(source: body|result, atom_ordinal_in_its_own_xdr_vec,
asset_sold, asset_bought, amount_sold, amount_bought)` — NOT from iteration /
   assembly position. A single shared library function (not duplicated in the
   backfill crate) called by both live-ingest and backfill, plus a differential
   test parsing a fixture set through BOTH paths asserting byte-identical rows.
   Until that exists, fan-out correctness is UNPROVEN and silent-corruption-prone.

## Also before the ADR (non-blocking)

- **Total op-arm → role mapping table** (all 26 `operation.rs` arms → role or a
  justified `N/A`; add clawback + a trustline-flag-target role; decide
  create-account / inflation; resolve PoolShare 3-entity keying).
- **State the parity contract:** endpoints validate 1:1 vs Horizon operation
  objects; `traded` roles validate vs Horizon `/trades`; no single external
  source validates the union (stellar.expert has no per-asset API to diff).
- **Re-derive loss counts on FINAL** and label every count "prod snapshot,
  date, FINAL".
- **Evaluate the bounded backfill** (distinct ledgers touching the defective op
  types) before signing off the row/backfill budget.
- **Native:** golden-pin `hash64("native")` on the backfill path; enumerate
  native's two keys (classic-leg surrogate + native-SAC surrogate) in the union
  predicate; add a native-specific Horizon cross-check.

## Corrections this pass forced on sibling notes

Applied to [S-field-comparison-fat-thin](S-field-comparison-fat-thin.md):

- "Adding a field later = the whole backfill again" — **wrong**. `ADD COLUMN` is
  metadata-only; the later mutation hardlinks untouched columns. It needs an S3
  re-parse only to SOURCE a new per-leg field's values, not a table rewrite.
- Tier-2 framed as "the real cost fork / an extra result/meta parse" —
  **overstated**. The `OperationResult` is already deserialized in the live
  parse; realized amounts + OrderBook trade atoms are marginal field reads.
- ZSTD "20–40×" (inherited from ADR 0044/0047 for `topics_xdr` JSON) does **not**
  transfer to `Decimal128` amounts / `price` / StrKeys — realistic single-digit
  ×; re-derive the fat/thin TB estimate from the 128 k-ledger fixture before any
  row-budget sign-off.

## Per-slice verdicts

| Slice                    | Verdict                                                                                        |
| ------------------------ | ---------------------------------------------------------------------------------------------- |
| Root cause + numbers     | Ship with changes (drop "THE root cause"; re-derive counts on FINAL; source "unbounded")       |
| Fan-out vs alternatives  | Ship with changes (fan-out defensible, not proven mandatory; Option 6 not refuted; FE is THIN) |
| Native surrogate         | Ship with changes (mechanism OK; native does NOT justify fan-out; enumerate two native keys)   |
| Backfill + provenance    | Ship with changes (mandatory-S3 HOLDS; evaluate bounded backfill; Tier-2 cheaper than framed)  |
| Fat/thin                 | Ship with changes ("re-backfill" false; ZSTD cherry-picked; per-column, not per-tier)          |
| Completeness ceilings    | Ship with changes ("hops redundant" BLOCKING-conditional; claim-asset in meta)                 |
| Scope / role / leg_index | **Rethink packaging** (split epic; leg_index content-addressed + diff test; total role table)  |

Related: [[feedback_sources_are_interpretations]], [[feedback_fundamental_complete_backward_data]],
[[project_native_two_conventions]], [[feedback_task_scope]].
