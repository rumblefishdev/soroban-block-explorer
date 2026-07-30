---
id: '0460'
title: 'FEATURE: transaction-detail polish — contract labels + small UX debts from 0453'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0453', '0297']
tags:
  [frontend, backend, transaction-detail, ux, priority-medium, effort-medium]
links: []
history:
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0453: the deliberate deferrals and small debts that did
      not merit their own tasks, grouped so they do not rot as prose.
  - date: '2026-07-30'
    status: active
    who: karolkow
    note: >
      Activated to ship items 8 (failed-banner redesign) and 10 (picker
      width) directly on develop per the post-ship review.
---

# FEATURE: transaction-detail polish

## Scope (each independently shippable)

1. **Contract display names** — stellar.expert shows `[Kale] CDL7…`; the
   name lives ON-LEDGER in instance storage `Symbol("METADATA")` (the
   off-ledger verdict in 0156/0283/0297 was chain-refuted — see project
   memory). Surface it in the invoke headline + Authorized calls tree.
2. **Picker mini-headlines** — rows currently repeat the type label; use
   the humanizeOp sentence (truncated) as the secondary line.
3. **Fee-bump fee breakdown** — SE shows non-refundable/refundable split;
   needs result-meta fee fields (may ride on 0457's enrichment).
4. **Pre-Protocol-23 note** — V3-meta txs have tx-level-only events; one
   quiet line in the events section instead of unexplained absence.
5. **Adaptive index** — the deferred layout option: hide the picker when a
   transaction has exactly one operation (87% of mainnet); revisit with
   the team.
6. **Toggle-as-preference decision** — the wave-5 question parked for
   evidence: an Etherscan-style "always expand raw" preference; add the
   cheap click-counter telemetry first (product call).
7. **XdrRow / DisclosureRow unification** — when a third disclosure-style
   consumer appears, not before.
8. **Failed-transaction banner redesign** — SHIPPED 2026-07-30 (develop):
   replaced the floating red box with a full-bleed thin status strip under
   the Summary header, borrowing the error Chip's palette (`surface.error`
   bg, `text.error` text + 16px ErrorOutline icon, `stroke.error` bottom
   border) so it stays in the house style without adding a MuiAlert theme.
   The "not applied" chip on the operation card is untouched.
9. **Story-chip placement**: next to the Success chip it reads as a second
   status — move next to the page title or the Operations header.
10. **Picker width** — SHIPPED 2026-07-30 (develop): grid is now
    `md: 5/7, lg: 4/8` (was 50/50); pairs with item 5, adaptive index.
11. **Clickable values in headlines/facts**: sentences are plain strings, so
    assets/accounts in them are dead text — needs ReactNode sentences;
    coordinate with 0456 (typed details → componentised templates).
12. **Route-strip edge labels**: hop amounts are the pools' actual payouts
    while the headline shows the min/max bound — label edges (e.g. "actual")
    so the two numbers read as different facts, not a mismatch.
    Finding (2026-07-30): `claimedAtoms` carries BOTH sides of each fill
    (`amountSold` + `amountBought`, `operation.rs::append_pool_claims`), and
    path-payment hops chain exactly (output of hop N = input of hop N+1) —
    so an order-book hop whose NEXT hop is a single pool fill can borrow
    that atom's `amountBought` as its own output (e.g. the missing VELO→XLM
    edge = the XLM the XLM/KALE pool took in). FE-only; only valid when the
    next hop has exactly one atom (multi-fill hops split the input).
    Better root fix (same day): `claim_atoms()` already returns the
    ORDER-BOOK atoms too (full sold/bought amounts + seller) — only
    `claim_lp_atoms` filters them out before `claimedAtoms` is built. Emit
    them with a `source: orderBook|pool` marker and every edge gets its
    amount directly; pool-volume consumers filter on `poolId`. Belongs to
    0457's scope (it owns fill amounts), supersedes the borrowing trick.
13. **Raw-data completeness** (post-ship review, 2026-07-30: "always keep the full raw
    data reachable as a fallback"): mostly already true after 0453 (op-card
    details disclosure renders every key; Events table collapsed; Raw data
    section always present). Two real gaps: (a) `result_meta_xdr` is parsed
    but not exposed on `heavy` — the ledger-entry changes are the one raw
    layer the page cannot show; add the DTO field + an XdrRow;
    (b) the XDR rows are bare base64 — add an "open in Stellar Lab viewer"
    deep link per row for one-click decoding (stellar.expert does this).
14. **Claimable-balance claimants are a bare count** (0462 review,
    tx `a821ee85…` op 3): the card says "for 2 claimants" but cannot name
    them — the PARSER reduces the claimants XDR vec to `claimants: 2`
    (`extract_op_details`, CREATE_CLAIMABLE_BALANCE arm). Backend fix: emit
    `[{destination, predicate}, …]`; then humanizeOp/facts list the
    addresses (keep count-compat while old parses linger). Not reachable
    from the frontend today.

## Acceptance criteria

- [ ] Each item shipped or explicitly withdrawn with a recorded reason
