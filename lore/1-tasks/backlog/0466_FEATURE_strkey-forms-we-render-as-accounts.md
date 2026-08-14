---
id: '0466'
title: 'FEATURE: strkey forms we render as accounts — muxed (M…) and claimable balance (B…)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0463', '0453', '0462']
tags:
  [
    frontend,
    backend,
    transaction-detail,
    strkey,
    priority-medium,
    effort-medium,
  ]
links: []
history:
  - date: '2026-08-07'
    status: backlog
    who: karolkow
    note: >
      Spawned from a CodeRabbit finding on the release PR: the execution
      trace classified every address argument as an account, so `M…` and
      `B…` rendered as links to pages that do not exist. The immediate
      fix (render them as plain text) stops the broken link but says
      nothing about why they are not clickable — this task closes the gap
      properly. Two forms, two different answers.
---

# FEATURE: the strkey forms we render as accounts

## Summary

SEP-23 defines eight strkey forms. We route three of them (`G` account,
`C` contract, `L` pool) and treat everything else as an account. Two forms
reach the UI today and are not accounts: **muxed accounts (`M…`)** and
**claimable balances (`B…`)**. They need different treatment, not one page
each.

## What we handle now

`web/src/pages/transaction-detail/op-card/strkeyDecode.ts` knows exactly
three version bytes:

```ts
export const STRKEY_VERSION = {
  contract: 2 << 3, // C
  account: 6 << 3, // G
  pool: 11 << 3, // L
};
```

The router matches the same three plus assets, NFTs, ledgers and
transactions. There is no `B…` route and no `M…` route.

## M… — not a missing page, a missing decode

A muxed account **is not a separate ledger entity.** It packs a `G…` account
together with a 64-bit sub-id (SEP-23). There is nothing to give a page to:
there is an account, and there is a sub-address number inside it.

We already decode this. `muxed_to_g_strkey` lives in
`crates/xdr-parser/src/envelope.rs:36` and is used **7 times** in
`operation.rs` alone, so operation sources and destinations arrive at the
frontend already collapsed to `G…`.

The one place it survives is **raw ScVal address payloads** — contract-call
arguments in the execution trace, where nothing normalises them. That is the
whole gap.

Wanted: decode `M…` to its underlying `G…`, link the account, and show the
sub-id beside it rather than dropping it (the sub-id is what distinguishes
one exchange customer from another — losing it silently is the same class of
defect as hiding a zero balance).

## B… — a genuine missing entity

A claimable balance **is** its own ledger entry: an id, an asset, an amount,
a claimant list with predicates, and a lifecycle (created → claimed, or
clawed back). stellar.expert gives them pages; we have nothing.

We already read part of this at parse time —
`asset_appearances::claimed_cb_asset_amount` recovers the asset and amount
from the same-operation ledger entry, which is how the card says
"Claimed 5 USDC". But the balance itself is not indexed, so a page needs
ingestion work first, not just a route.

Scope this before committing to it: count how often `B…` actually appears in
what we render, and decide between a full detail page and a modest inline
disclosure on the operation that touched it.

## Two more forms, deliberately not pages

`P…` (pre-authorised transaction) and `T…` (hash(x)) are **signer keys**, not
addresses. They appear in `SET_OPTIONS` and will appear on the account detail
page when task 0463 surfaces signers. Neither should ever get a page — but
neither may be rendered as an account link either, which is exactly the
mistake this task exists to close. Today `humanizeOp` prints them as plain
text, which is correct; 0463 is where the risk returns.

`S…` is a secret seed. It must never appear anywhere. Worth an explicit
guard rather than an assumption.

## Interim state (already shipped)

`ExecutionTrace.tsx` now links only `G` / `C` / `L` and renders every other
address form as plain shortened text. That removes the broken link but is
silent about the reason — the reader cannot tell "we chose not to link this"
from "this is not linkable". Treat it as a stopgap, not the answer.

## Acceptance criteria

- [ ] `M…` in any rendered position resolves to its underlying `G…` account
      link, with the sub-id shown, never dropped
- [ ] The muxed decode is shared with the backend's `muxed_to_g_strkey`
      rather than reimplemented (the 0431 lesson)
- [ ] A decision recorded for `B…`: detail page, inline disclosure, or
      declined — with the measured frequency behind it
- [ ] No strkey form renders as an account link unless it is one; unknown
      forms are visibly not links rather than quietly unclickable
- [ ] `P…` / `T…` signer keys stay text on the account page (guard added
      alongside 0463 rather than after it)
- [ ] **Docs updated** — `docs/architecture/frontend/frontend-overview.md`
      if the identifier-rendering contract changes
- [ ] **API types regenerated** — only if a DTO gains a field; a pure
      frontend decode needs none
