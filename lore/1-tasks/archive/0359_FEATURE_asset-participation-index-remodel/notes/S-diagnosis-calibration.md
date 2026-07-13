---
title: 'Diagnosis calibration — thesis, red-team, Stanisław corroboration'
type: synthesis
status: mature
spawned_from: '0359'
spawns: []
tags: ['calibration', 'red-team', 'diagnosis']
links: []
history:
  - date: 2026-07-06
    status: mature
    who: karolkow
    note: 'Calibrated diagnosis: origin thesis + 4-agent red-team corrections + Stanisław corroboration + code-verified second-slot check. Extracted from the 0359 README on folder conversion (2026-07-07).'
---

# Diagnosis calibration

## Origin & calibrated thesis (post devil's-advocate)

Discovered chasing 0348/F2. Initial framing ("single-slot fundamentally breaks
per-asset lists") was **stress-tested against prod CH and partly walked back** —
recorded honestly here so we build the right thing:

- **Issued-asset lists are NOT broken.** The top-5 issued assets by volume
  (yXLM, USDC, AQUA, HELIX, SSLX) each surface **4 op types `[1,2,13,6]`** =
  Payment + PathPayment(receive) + PathPayment(send) + ChangeTrust. Users DO
  see varied, useful activity.
- **The acute bug is narrow:** only **native XLM** was fully empty
  (`GET /assets/native/transactions` → `{"data":[]}`), because native is the
  only asset with no positive key. Confirmed live + at `queries_ch.rs`
  (early-return when neither classic code+issuer nor contract identity present).
- **The systematic completeness gap** (affects all assets): **offers carry zero
  asset identity** and **path-payments store at most one of their legs**.

So this task = **fundamental completeness + native-first-class**, not "repair a
broken feature". Scope chosen deliberately (karolkow): do it right at the root.

## Independent corroboration — Stanisław Królikiewicz (stkrolikiewicz), 2026-07-06

Independent analysis by the senior, scoped to the L1 classic asset-attribution
question, **matches this task's core** (same prod numbers, same diagnosis, same
fix options) — a strong cross-check:

- **"Native has zero operations" → REFUTED (agrees).** Native IS written
  (`asset_code=''`, `asset_issuer_id=NULL`, from `split_asset_ref("native") →
(None,None)`); **514.09 M** type-1 native payments exist. The "zero" is purely
  read-side: `/assets/:id/transactions` is out-of-scope for native (documented in
  `10_get_assets_transactions.sql`) and any `asset_code='XLM'` query returns 0
  because native is the empty string. = our K2-1.
- **Multi-asset loss → CONFIRMED (agrees).** `OpTyped::from_details` keeps one
  asset slot; RMT key `(ledger, tx, application_order)` has no asset → >1 asset
  per op architecturally impossible. Offers (SellOffer 771.6 M + BuyOffer 598.6 M
  - PassiveSell 0.13 M ≈ **1.37 B**) hit the parser `_` fallthrough → **no leg
    attributed**. Path-payments keep only `destAsset`, drop `sendAsset` → every one
    loses its **source leg** (type 2: 1.07 B, type 13: 0.67 B ≈ **1.74 B**). = our
    K1-1/K1-2.
- **New detail added:** **Claimable balances (~11.7 M)** — CreateClaimableBalance
  - ClawbackClaimableBalance also drop their asset. (Fold into the L1 re-model.)
- **Partial compensation confirmed:** `pool_ids` recovers AMM-pool-crossing
  path-payments/offers (asset via pool join); order-book crossing + source leg
  stay invisible. = agent "C2".
- **Type-read validated:** op types that can't be native (Clawback/AllowTrust/
  SetTrustLineFlags) never appear with empty `asset_code` → the `type` numbers are
  read correctly. Rules out a mis-decode explanation.
- Fix options he lists (2nd asset column / `operation_asset_participation` table /
  fan-out per (op × asset)) = the same options in this task's design. His framing:
  "billions of rows = not an accident, it's a property of the project."

## Red-team calibration (2026-07-06) — corrections to the audit above

A 4-agent adversarial red-team (each told to REFUTE, re-derive numbers on prod,
flag intended-by-design) stress-tested every finding. The core holds but several
figures/severities were **overstated**; recorded here honestly. These corrections
**supersede** the numbers/severities in the tables above.

**Overstatements corrected:**

- **Headline "~3.1 B / 48% lost" → ~2.4 B / 37.5% carry NO asset.** Path-payments
  were counted whole (1.734 B), but **42% (731 M) keep `destAsset`** — a valid
  per-asset entry, not a loss. Honest floor = offers 1.37 B (21.4%) + empty
  path-pay 1.003 B + claimable 26.9 M = **2.40 B (37.5%)** with zero asset. Of
  that, **offers (21.4%) are the unambiguous defect**; path-payment source-leg
  loss (15.7%) is a design-tradeoff, not a clean loss. `pool_ids` does NOT
  recover it (offers 0%, path-pay 4.5%/14.7%).
- **K2-1 (native tx empty): HIGH → LOW / by-design.** Documented out-of-scope,
  PG parity (`10_get_assets_transactions.sql:61-62`). Data exists; read-side
  choice, not a defect. (Still worth doing as part of the re-model, but not a
  "bug".)
- **K3-1 (SAC not unioned): HIGH → MEDIUM.** SAC activity IS reachable via the
  contract-transactions endpoint (unions `soroban_invocations_appearances` by
  contract*id); it's a cross-reference gap on the \_asset* page, not invisibility.
- **K2-2 (LP native leg): 16,552 / 701 → 11,641 pools (22.4%) / 480 impostors.**
  Our count was raw ReplacingMergeTree rows; FINAL (user-visible) is ~1.4× lower.
  Mechanism airtight; HIGH stands; % actually slightly higher than claimed.
- **K1-3 (events undecoded): reword.** Core holds (`parse_transfer` is dead code;
  no queryable from/to/amount column; API `amount` hardcoded 1). But "9.5 B
  opaque/undecoded" overstates: `signature` IS a queryable column, and
  `topics_xdr`/`data_xdr` are ScVal-decoded JSON (not opaque XDR) — from/to/amount
  are recoverable from the payload, just not promoted to named columns.
- **K3-4 (events not unioned): downgrade + correction.** G-sided transfers DO
  appear on account pages via the `transaction_participants` back-fill; the gap is
  non-G sides + mint/burn/clawback, not "all transfers absent". The earlier claim
  that this was a "deliberate documented quota decision" is **unsubstantiated —
  no such ADR/doc exists** (retracted).
- **K2-3 (non-G participants): C/B/L only; muxed-M is NOT dropped** (canonicalised
  to base G upstream, ADR 0026). Dropping C/B/L is intended for the accounts-shape
  index; the real gap is contract/CB/LP transfer participation invisibility.

**Reclassified as NOT bugs (remove from the defect count):**

- **DEX per-asset trades** — an unbuilt feature (scope/roadmap), not a defect.
- **K3-5 (Soroban-AMM pools not unioned)** — INTENDED-BY-DESIGN, product-gated
  deferral (ADR 0014 §, ADR 0017 deferred-topics).
- **K4-5 (nullable-aggregate 500 trap)** — theoretical; the guards (`ifNull`,
  `toString`→Option, non-Nullable `count()`) are already present in all live CH
  `fetch_one` sites. Keep only as a review-time watch note.
- **K4-3/K4-4 (amount=1 / fold-count)** — documented naming choice, not a
  divergence bug (a "confusing field name" note at most).

**More corrections (quick-verdict cluster):**

- **K2-8 (contract-holder orphan): MED → LOW.** The primary SAC-`ContractData`
  `BalanceValue` path for contract-held classic/native IS shipped and
  prod-validated (`state.rs:302-369,428`; wired `process.rs:420` → `balances`);
  task 0331 (done) closed it. The earlier "types 0/1/2 skipped" framing came from
  0331's **pre-implementation** notes. Residual tail only: frozen-balance flags
  not propagated + non-standard custom-storage tokens skipped (not mis-summed).
- **K1-4 (op fold < operation_count): CONFIRMED but INTENDED-BY-DESIGN** —
  documented (`dto.rs:188-194`); heavy `operation_tree` carries the full unfolded
  list. Cosmetic on the light array.
- **K1-5 (crossed-offer counterparty dropped): CONFIRMED, COMMON** — order-book
  crossing is the normal taker path (`operation.rs:118-121` drops
  `ClaimAtom::OrderBook` seller_id). Solid MED.
- **K1-6 (NFT single-owner) / K3-7 (NFT collection): LOW confirmed** — correct for
  the indexed single-owner standards (no ERC-1155 path); collection-name sourcing
  fixed by task 0340.

**Overall-thesis verdict: SHIP WITH CHANGES.** "Single participant slot is _a_
real design limitation with real consequences (offers = 1.37 B rows, zero asset
attribution)" is **solid and HIGH**. But "single-slot is _the_ single root cause"
overclaims — native (out-of-scope) and SAC-union are **separate, independently
documented choices**, not downstream effects of the asset slot. The "codebase
already models participants/pools multi-valued elsewhere" contrast IS accurate
(not cherry-picked): `op_participant_str_keys` extracts all 3 asset fields into
`transaction_participants`; `pool_ids` is a real `Array`.

**Still quota-blocked (re-derive after 13:00 UTC):** fee-bump 45% (verify it's
not head-partition-only), NFT contract-owner 22%/51%, pending NFTs 71K,
contract_ids 100%-of-Soroban-txs. Mechanisms confirmed in code; exact prod
percentages pending the CH read-quota reset.

**Net after calibration:** the genuine, confirmed HIGH core is **offers carry
zero asset attribution (1.37 B / 21.4%)** plus the read-side native/SAC
completeness gaps (now LOW/MED) and the L2 fungible-transfer decode gap. Fewer
clean HIGHs than first stated; the re-model is still justified by the offers
defect + native-first-class goal, but sized honestly at **~37.5% no-asset**, not
48%.

## Code-verified cross-validation of Stanisław's second-slot proposal (karolkow, 2026-07-07)

Stanisław (stkrolikiewicz) sent a follow-up recommending a **second asset slot**
(`asset_code_2`) over the fan-out participation table this task proposes. Every
one of his code-verifiable claims was **checked against source and CONFIRMED** —
he agrees with this task on the diagnosis and diverges only on the fix mechanism.

**Claims verified in code (all ✅):**

- **Latent debt, not a signed scope decision.** Confirmed: this task already
  retracted the "documented quota decision" (line ~372). ADR 0011:754 literally
  says _"multi-asset filter semantics (single asset OR vs asset pair AND) to be
  defined"_; ADR 0044 is silent on the single-asset slot. Only native-out-of-scope
  and no-amount-extraction (ADR 0029) were ever consciously decided.
- **Raw parse keeps ALL legs in details JSON; the column projection picks one.**
  `crates/xdr-parser/src/operation.rs:216-235` emits `sendAsset` + `destAsset` +
  full `path[]`; offers emit `selling` + `buying` (238-266); CreateClaimableBalance
  emits `asset` (344). The loss is 100% downstream at the projection.
- **path-payment keeps only `destAsset`.** `crates/db-clickhouse/src/persist/stage.rs:1757`
  reads `destAsset` only; `sendAsset` + `path[]` dropped.
- **Offers hit the `_` fallthrough.** No projection arm for ManageSell/ManageBuy/
  PassiveOffer → they fall into `_ => {}` at `stage.rs:1806` → asset_code=None.
  Parser has them (JSON carries both legs); projection reads **zero** of them.
- **CreateClaimableBalance drops its asset** the same way (no arm → `_`).

**Nuance he omits:** the free CreateClaimableBalance forward-fix works because its
details carry `asset`. But **ClaimClaimableBalance carries only `balanceId`, not
the asset** (`operation.rs:352`) — claim/clawback-CB can't be forward-fixed the
same cheap way (needs a CB-id→asset lookup). The freebie is narrower than stated.

**The one real divergence — second slot vs fan-out:**

|                                      | Second slot (Stanisław)                            | Fan-out table (this task)                    |
| ------------------------------------ | -------------------------------------------------- | -------------------------------------------- |
| Diff                                 | tiny: parser arms + 2 cols + 1 bloom + 1 OR filter | new table, 3-stream union, keyset pagination |
| RMT dedup key                        | **unchanged** → dedup + amount(fold) untouched     | grain change → risk on 6.4 B                 |
| Row blow-up                          | none                                               | multiplies rows >6.4 B                       |
| Backfill                             | re-stage (0266/0304 patterns)                      | full XDR re-parse fan-out                    |
| path hops (`path[]`, up to 5 assets) | ❌ can't hold (only 2 slots)                       | ✅ arbitrary N                               |
| native first-class                   | ❌ still `asset_code=''` absence                   | ✅ positive surrogate (0331/ADR 0051)        |

**Arbitration** — hinges on two goals this task's own red-team already softened:

1. **path hops** — this task wants them (line ~143), but nobody filed them as a
   defect. YAGNI → second slot suffices.
2. **native first-class** — the ORIGINAL bug (0348/F2). The second slot does
   **not** fix it (native stays empty-string). But the red-team downgraded native
   to **LOW / by-design** (line ~355). If native = a read-side choice, slot is
   enough; if native-as-surrogate is a hard requirement, slot is insufficient and
   the 0331 surrogate work is needed regardless.
3. **offers** (the single CONFIRMED HIGH, 1.37 B) — **both** approaches carry the
   two legs (selling→slot1, buying→slot2). Not a discriminator.

**Synthesis (lazy-correct sequencing, matches Stanisław):**

1. Forward-fix the projection now, zero schema: add the CreateClaimableBalance arm
   (1 slot). Ship immediately — both agree.
2. Second slot for offers + path-payment source leg — covers **every CONFIRMED
   defect** (offers 1.37 B + source leg) at a fraction of the cost; RMT untouched.
3. Fan-out is justified **only if** native-as-surrogate and path-hop indexing are
   firm goals — the red-team weakened both, so today they are not.
4. Backfill = a separate decision (re-stage forward-fix cheaply; backward
   completeness gated on actual need), not the monolith this task currently frames.

**Net:** Stanisław's second slot delivers the confirmed HIGHs (offers + source
leg) for a tiny diff and leaves the RMT dedup/amount semantics untouched. The
fan-out table this task chose buys native-first-class + path-hop granularity —
requirements this task's own red-team already downgraded to LOW/by-design. The
open architectural call for karolkow: **native = positive surrogate (fan-out
needed) or read-side fix enough (second slot wins)?** See
[[project_native_two_conventions]].

> **Resolved (2026-07-08).** karolkow chose the **fan-out** as the end-state — but
> the reasoning shifted: the devils-advocate pass ([S-devils-advocate](S-devils-advocate.md))
> showed native does NOT justify the fan-out (it is read-side-fixable, shipped as
> **Phase 0** with no schema change), and the fan-out now stands on the unbounded
> result-side trades + hot-key seek, not native-first-class. Native still becomes
> a positive surrogate in the fan-out for consistency, but that is no longer the
> reason the schema changes. Sequencing: Phase 0 (native read-side + F-F) → Phase
> 1 (offers) → Phase 2 (full fan-out). The second-slot option is superseded.
