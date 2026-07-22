---
id: '0317'
title: 'FEATURE: classifier 80/20 — monitored-UNKNOWN + launchpad-NFT discriminator (drain the pending residual)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0309', '0294', '0308', '0303', '0392']
tags:
  [
    parser,
    classifier,
    nft,
    completeness,
    sep-48,
    layer-data,
    priority-medium,
    effort-medium,
  ]
links: []
history:
  - date: 2026-07-22
    status: backlog
    who: karolkow
    note: >
      **New measured evidence from 0392 — and the framing shifts.** 0392 removed
      the `nfts_pending` quarantine (ADR 0053), so "drain the pending residual"
      in this task's title no longer describes a real operation: an unclassified
      contract's rows now sit in `nfts` and are hidden by a read-time verdict
      filter. Improving the classifier still has exactly the payoff this task
      claims — every contract it newly resolves to `Nft` makes its rows visible
      on the next read, with no migration.
      The sharper finding, measured on prod 2026-07-21 while checking that:
      **122 contracts carry an `Nft` verdict but only 66 have any NFT rows at
      all.** Of the 56 with none, 19 emit real events — 622 in total, including
      `mint` (46), `uri_upd` (58), `minted` (5), `identity_minted` (5),
      `transfer` (3) and 470 with no decoded signature. Those rows were never
      created: the parser does not recognise these shapes, which is the
      custom-ABI / launchpad gap this task owns. No amount of classifier work
      surfaces a row that was never written — the parser side is the binding
      constraint for those 19 collections, and it is invisible to every
      classification-side metric.
  - date: 2026-06-23
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0294 deep-dive on nfts_pending redundancy. The pending
      queue is NOT redundant — it is load-bearing ONLY because the WASM
      classifier is name-only and returns `Other` for bespoke NFTs. Measured
      residual is tiny (~65 contracts = ~7 WASM templates). This task is the
      actionable 80/20 increment of the strategic 0309 rebuild; the full L0-L4
      total-function (option A) stays in 0309.
---

# FEATURE: classifier 80/20 — monitored-UNKNOWN + launchpad-NFT discriminator

## Summary

`classify_contract_from_wasm_spec` (`crates/xdr-parser/src/classification.rs:101-120`)
is a name-only matcher over 5 NFT names. Bespoke NFTs whose WASM exposes none of them
classify `Other` forever, so their NFT-candidate events sit in `nfts_pending` and never
drain (the table's only remaining load-bearing reason — see auto-memory
`project-nfts-pending-load-bearing-classifier`). This task closes that gap with the
minimum change, and makes the `Other` bucket **observed** instead of silent.

## Measured residual (prod `chq`, 2026-06-23)

Distinct `nfts_pending` contracts that are `Other` WITH a WASM = **65 contracts across
only ~7 WASM templates**. They are heterogeneous:

- **Launchpad-NFT family** (custom ABI, genuine NFTs): functions `get_token_info`,
  `get_all_owned`, `get_max_token_id`, `update_token_url`, `bulk_mint`, `mint_original`,
  `is_collection_frozen`, `update_collection_info`, `freeze_collection`,
  `get_collection_info`. token_id / collection semantics. **Should classify `Nft`.**
  (auto-memory `custom_abi_nft_class_missed`.)
- **Custom financial / RWA** (NOT NFTs): functions `pay_off`, `sign_off`, `redeem`,
  `loan_status`, `set_loan_contract`, `add_vc`, `vc`, `check_paid`, `check_expired`.
  Emit `transfer`/`mint` with i128 **amounts** (not token_ids) → their pending rows are
  FALSE NFT candidates.

The other ~5,215 pending contracts (no-WASM, NULL verdict) are un-deployed SACs handled
by the task-0294 detection gate — out of scope here.

## Design (option B — the 80/20)

1. **Monitored-UNKNOWN.** Make `Other` non-silent: a `%UNKNOWN` metric + a per-WASM-template
   tripwire when a contract emits NFT-candidate events yet classifies `Other`. The pending
   table IS the unknown bucket; this only adds observability so new templates surface
   instead of rotting. (Core of 0309's "never SILENTLY miss".)
2. **Launchpad → `Nft`.** Add the launchpad discriminator names to
   `classify_contract_from_wasm_spec` so the family classifies `Nft` and `nft_reclassify`
   promotes its pending → hot. Prefer a small, named discriminator set (and consider
   signature-based matching) over a fragile single name.
3. **Custom-RWA verdict — OPEN DECISION** (how to drain their false pending):
   - (a) give them a `Fungible`/`Token` verdict (route_for → Drop) — data is amounts.
   - (b) keep `Other` but route `Other` + i128-amount-shaped data → Drop (data-type
     discriminator at NFT detection, not just WASM).
   - (c) leave `Other` → Pending but **monitored** — accept a handful of custom-RWA in the
     observed unknown bucket (API never reads `*_pending`, so harmless).
   - **Lean: (c)** — don't fabricate a verdict for contracts we don't fully understand;
     observe them. Decide at implementation.

## Out of scope

- Option **A** — the full L0-L4 total-function rebuild (SEP-48 spec-driven decode + SEP-46/47
  capability + typed-shape cascade) — stays in **0309** (effort-large, deferred; near-term
  value low since SEP-48 adoption ≈ 0). This task is the increment that captures most value now.
- Removing `nfts_pending` — premature until the residual is drained AND the classifier is
  total; see the deep-dive verdict (memory `project-nfts-pending-load-bearing-classifier`).

## Acceptance Criteria

- [ ] `Other`-with-NFT-events is observable (metric + tripwire), not silent
- [ ] Launchpad-NFT family classifies `Nft`; its pending promotes on `nft_reclassify`
- [ ] Custom-RWA pending drained or explicitly accepted as monitored-unknown (decision a/b/c)
- [ ] New classifier behavior unit-tested against the ~7 residual WASM shapes
- [ ] Docs: `classification.rs` doc updated to state `Other` is monitored, not silent

## Docs updated

- N/A at spawn time — fill `docs/architecture/xdr-parsing/*` (classifier behavior) when implemented.
