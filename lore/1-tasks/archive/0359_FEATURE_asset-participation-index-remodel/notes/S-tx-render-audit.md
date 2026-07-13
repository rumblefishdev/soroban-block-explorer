---
title: 'Transaction-detail render audit (normal vs advanced) — /ux-expert + live inspection'
type: synthesis
status: developing
spawned_from: notes/R-prod-evidence-cross-validation.md
spawns: []
tags:
  ['frontend', 'ux', 'transaction-detail', 'ux-expert', 'spawns-separate-task']
links: []
history:
  - date: 2026-07-08
    status: developing
    who: karolkow
    note: >
      Live inspection of the running front (localhost:4200) for tx df80d042 in
      both normal + advanced modes, root-caused in code, audited with /ux-expert.
      This is a SEPARATE frontend/UX concern from 0359's data model — recorded
      here because it surfaced during the 0359 investigation; it should spawn its
      own FE task (on develop).
---

# Transaction-detail render audit

## Scope + relation to 0359

This is about the **transaction-detail operation render** (`web/src/pages/
transaction-detail/`), NOT the per-asset index. 0359 fixes the _data_ (queryable
per-asset participation); this fixes _how one transaction's operations are shown_.
They are independent — but 0359's richer data would feed a better render.
**Recommendation: spawn a separate FE/UX task (on develop).**

## Live example (inspected 2026-07-08, localhost:4200)

Tx `df80d042…` — a `PATH_PAYMENT_STRICT_SEND` self-swap: GAFB…36GD swaps its own
**1 XLM → bubba**, routed via **TF**, across 2 liquidity pools. Ground truth
(Horizon + the advanced panel's `claimedAtoms`): 3 assets (native, TF, bubba).

**Normal mode shows:** a source→dest flow + one line — **"Result: Sent 1 XLM to
GAFB…36GD"**. It shows only the send leg (native), hides the received `bubba`, the
`TF` hop, and the pools. Source == destination, so "Sent 1 XLM to [self]" is
meaningless.

**Advanced mode shows:** a raw key-value dump of internal field names —
`claimedAtoms` (JSON), `destMin`, `poolIds`, `sendAsset`, `sendAmount` (raw
stroops `10000000`), `destAsset`, `path` — plus an Events table of raw ScVal JSON
and raw `envelope_xdr` / `result_xdr`. Complete but developer-facing.

Neither renders like stellar.expert's clean _"swapped 1 XLM → TF → X bubba"_.

## Root cause (code)

- `normal/humanizeOp.ts:52-61` — `amountFieldsFor()` maps
  `PATH_PAYMENT_STRICT_SEND` to `sendAmount`/`sendAsset`, so line 111 emits
  **"Sent {sendAmount} {sendAsset} to {dest}"**. The comment even admits it: the
  op "carries no actual destination amount (only destMin), so it falls back to
  what the source committed". But the **received** amount IS available — in the
  result-side `claimedAtoms` (last atom `assetSold=bubba`,
  `amountSold=3383190106609232`). The render simply doesn't use the result.
- `normal/toFlowNodes.tsx` — builds Source → (Sends to) → Destination → Result.
  Prefers backend `details.summary_line_1/2` when present; here they're absent so
  it falls to `humanizeOp`. Assets are not first-class nodes; the route is absent.
- `advanced/OperationJsonDetail.tsx` — generic `AdvancedRow(label, value)` dump of
  `details` keys; no humanization, raw stroops, internal names.

## UX audit (/ux-expert) — findings, biggest impact first

| #   | Finding                                                                                                        | Severity            | Principle                                                        |
| --- | -------------------------------------------------------------------------------------------------------------- | ------------------- | ---------------------------------------------------------------- |
| 1   | Normal one-liner is **factually misleading** for path-payments/swaps ("Sent 1 XLM" for a bubba swap)           | **Critical**        | A summary must be TRUE first; a wrong summary is worse than none |
| 2   | Render is organized around **accounts** (source→dest nodes), not **asset movement** (what moved, how much)     | Major               | Group by user mental model, not structure                        |
| 3   | The **route / hops / pool crossings** (the whole point of a path payment) are invisible in normal              | Major               | Show the thing the operation is about                            |
| 4   | Advanced = **raw dump** of internal field names + raw stroops; reads as a leaked debug panel                   | Major               | Even "raw" should format amounts + use human labels              |
| 5   | The **normal↔advanced binary** gives two mediocre views (too little / too much) instead of one progressive one | Major               | Summary first, details on demand (Shneiderman)                   |
| 6   | **Received amount is discarded** though present in `claimedAtoms`                                              | Major               | Use the data you already have                                    |
| 7   | **Self-transfer** (source==dest) not recognized → "to [same account]"                                          | Minor               | Recognize special cases                                          |
| 8   | Events table is **raw ScVal JSON**; no direction/color on amounts                                              | Minor / Enhancement | Humanize; encode direction with sign + color                     |

## Proposed redesign (one progressive operation card)

Drop the hard normal/advanced binary. One card per operation: a TRUE human
headline, key facts, then progressive-disclosure detail; raw XDR at the bottom.

```
┌─ Operation 1 · Path Payment (Strict Send) · Classic ───────────┐
│  GAFB…36GD swapped  (self)                                     │
│     1 XLM   →   via TF   →   3,383,190.10 bubba                │
│                                                                │
│  Sent      1 XLM                                               │
│  Received  3,383,190.10 bubba                                  │
│  Route     XLM → TF → bubba      · 2 pools                     │
│  Min recv  3,206,685.74 bubba    (slippage bound)             │
│                                                                │
│  ▸ Trades (2)     XLM ← 521.46 TF · TF ← 3.38M bubba          │
│  ▸ Token events (6)                                            │
│  ▸ Raw XDR (envelope · result)                                │
└────────────────────────────────────────────────────────────────┘
```

- **Headline** = a true sentence built per op type (swap chain for path-payments;
  "sent X ASSET to Y" only for real single-asset payments; "sold X for Y" for
  offers). Never show only the send leg for a swap.
- **Key facts** = Sent / Received / Route / slippage — the asset movement, first.
- **Progressive detail** = Trades (formatted `claimedAtoms`), Token events
  (humanized SAC transfers), Raw XDR. This absorbs today's "advanced" without a
  separate mode.

## Spec essentials (for the FE task)

- **Data mapping:** Sent = `sendAsset`/`sendAmount`; Received = final
  `claimedAtoms` atom's delivered asset/amount (or `destAmount` for strict-receive);
  Route = `[sendAsset, ...path, destAsset]`; Trades = `claimedAtoms` (assetSold/
  amountSold ↔ assetBought/amountBought + poolId/offerId); Events = decoded
  `soroban_events` transfer topics. All already present in the heavy `details`.
- **Reuse:** `formatTokenAmount`, `truncateMiddle`, `Chip`, `FlowNode` (for the
  route chain), existing expandable-section pattern from `RawDataSection`.
- **Per-op-type headline** replaces `humanizeOp`'s send-only branch; fix the
  `PATH_PAYMENT_STRICT_*` mapping to use the result for "received".
- **Correctness fix (ship first, independent):** stop emitting "Sent {sendAmount}
  {sendAsset} to {dest}" for path-payments — at minimum "Swapped {send} → {recv}".

## Next

Spawn a FE/UX task on develop: "Transaction operation render — human summary +
progressive detail (replace misleading normal one-liner + raw advanced dump)".
Link `related_tasks: [0359]`. The `humanizeOp` path-payment mislabel is a
shippable correctness fix on its own.

## More op-type examples (verified live, 2026-07-08)

Checked our front (normal + advanced) vs stellar.expert across op types:

| Op                | Our NORMAL                                         | Our ADVANCED                                           | stellar.expert                                            |
| ----------------- | -------------------------------------------------- | ------------------------------------------------------ | --------------------------------------------------------- |
| Path payment (13) | ❌ "Sent 1 XLM to [self]" (drops received + route) | ✅ full: sendAsset/destAsset/path/poolIds/claimedAtoms | ✅ "swapped X → Y" + Trades                               |
| Sell offer (3)    | ❌ "Manage Sell Offer processed" (nothing)         | ✅ full: selling ETH, buying USDC, price, offerId      | ✅ "offer to sell X ETH for USDC @ price"                 |
| Claim CB (15)     | ❌ "…processed"                                    | ⚠️ asset-BLIND: only `balanceId`                       | ⚠️ also "claimed balance [id]" (asset needs effects/meta) |

- **`humanizeOp` only handles PAYMENT / PATH_PAYMENT / INVOKE / CREATE_ACCOUNT.**
  Everything else → "{opLabel} processed" (offers, LP, claimable, clawback,
  change_trust, merge all render as "X processed"). Big gap.
- **Advanced already has the full parsed `details`** (archive re-parse) for most
  op types → the render fix is mostly HUMANIZATION of data we already fetch.
- **Exception — claim/clawback-CB asset is missing even in advanced** (op body
  carries only `balanceId`); needs same-op `LedgerEntryChanges`. A gap shared
  with 0359 Phase-2, and even stellar.expert's simple render shows only the id.
- **Status renders correctly** — verified our "Failed" == Horizon `successful:false`
  == stellar.expert "Failed" (tx d8b4bab5). No status bug (earlier suspicion retracted).

## Does the tx render need the fan-out? (schema inference)

- **Per-transaction render: NO.** It only needs good humanization of the already
  re-parsed op `details` (which advanced already holds). Independent of 0359.
- **Per-asset pages + a Trades tab: YES** — those need the fan-out + a trades
  table (see [R-prod-evidence-cross-validation](R-prod-evidence-cross-validation.md)).
  So stellar.expert's per-asset surfaces need the fan-out; its per-tx render does not.
- **What to borrow from stellar.expert:** (1) verb-based per-op sentence,
  (2) the route chain (X → hop → Y), (3) a Trades section from `claimedAtoms`,
  (4) inline per-asset amount + direction. All buildable from data we already
  parse (except the claim-CB asset = meta).

## Per-op-type render spec (option B)

Headline sentence + a "key facts" block, then progressive sections. All fields
come from the heavy `details` we already fetch (except where noted = meta).

- **payment:** "{src} sent {amount} {asset} to {dest}" (self → "…to itself").
- **path*payment*\*:** "{src} swapped {sendAmount} {sendAsset} → {recvAmount}
  {destAsset}" + **Route:** sendAsset → …path → destAsset. `recvAmount` = final
  `claimedAtoms` atom (or `destAmount` for strict-receive). **Never** "Sent
  {send} to {dest}".
- **manage_sell/buy_offer & passive:** "{src} offered to {sell/buy} {amount}
  {selling} for {buying} @ {price}" (+ `offerId`); if it crossed → "…, filled N
  trades" + Trades section. Passive: no `offerId`.
- **change_trust (asset):** "{src} opened/updated a trustline to {asset} (limit
  {limit})"; limit 0 → "removed trustline to {asset}".
- **change_trust (pool):** "{src} opened a trustline to pool {A}/{B}".
- **allow_trust / set_trustline_flags:** "{issuer} {authorized/froze} {trustor}'s
  {asset}".
- **create_claimable_balance:** "{src} escrowed {amount} {asset} for {N}
  claimant(s)".
- **claim_claimable_balance:** "{src} claimed {amount} {asset}" — asset+amount
  from **meta**; until that lands, "claimed balance {id}".
- **clawback / clawback_cb:** "{issuer} clawed back {amount} {asset} from
  {holder}".
- **account_merge:** "{src} merged into {dest} ({amount} XLM moved)".
- **create_account:** "{funder} created {new} with {startingBalance} XLM".
- **liquidity_pool_deposit:** "{src} deposited {amtA} {A} + {amtB} {B} into pool
  {A}/{B}".
- **liquidity_pool_withdraw:** "{src} withdrew {amtA} {A} + {amtB} {B} from pool
  {A}/{B}".
- **invoke_host_function:** "{src} called {fn}() on {contract}" + inner SAC
  transfers (from events) as humanized sub-lines.
- **set_options / manage_data / bump_sequence / sponsorship (16-18):** short
  factual line, no asset ("set options", "set data {k}", "bumped sequence",
  "sponsored reserves for {acct}").

**Layout:** headline → key-facts (Sent / Received / Route / participants — asset
movement first) → progressive sections (Trades from `claimedAtoms`, Token events
humanized, Raw XDR). The normal/advanced toggle collapses into progressive
disclosure on one card. Per-op-type headline replaces `humanizeOp`'s 4-case
switch; the mapping mirrors the role mapping in
[G-schema-and-roles](G-schema-and-roles.md).
