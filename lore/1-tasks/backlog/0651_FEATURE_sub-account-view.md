---
id: '0651'
title: 'Sub-account view: show only the transfers of an M… / W… sub-account'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0649', '0543', '0540']
tags: [api, frontend, muxed, effort-medium, priority-medium]
links: []
history:
  - date: '2026-10-19'
    status: backlog
    who: karolkow
    note: >
      Asked for by the owner during the review of #674 (task 0649): a
      pasted M… / W… should be able to show that sub-account's own
      transfers, not the whole account or contract.
---

# Sub-account view

## Summary

After task 0649 a muxed address (`M…` an account, `W…` a contract, each
with a 64-bit sub-account id) is answered as its base address: the account
or contract page and the transaction filters show everything of the base
address, and the sub-account id is dropped. A user who pastes the `M…` of
their exchange deposit sees the exchange's whole history, not their own
deposits. This task adds a view of one sub-account's transfers.

## Stan teraz

- Done: nothing yet.
- Next: decide the shape (grilling) after 0649's PRs merge.
- In force: 0649 decisions — `M…`/`W…` resolve to the base address.

## Context

The data exists: `asset_transfers.from_muxed_id` / `to_muxed_id` hold the
sub-account id of a transfer (task 0540 for classic operations, 0649 for
Soroban calls and `W…`). On 2026-10-19 production held 219,552 rows with
`to_muxed_id` out of 5.87 bn. 171 historic Soroban rows lack it until
0649's targeted backfill runs. No API endpoint reads these columns today,
and no transaction table carries a sub-account id, so the view is of
transfers, not of transactions.

## Open questions (for grilling)

- Where: a mode of the account / contract page when reached from a muxed
  address, or a separate route.
- API: a new endpoint, or a `muxed_id` filter on an existing transfer list.
- Sender side too (`from_muxed_id`), or recipient only.
- Memo-based deposits (an exchange's id memo, kept in `transaction_memos`)
  in the same view, or not.

## Acceptance Criteria

- [ ] A pasted `M…` / `W…` can show only that sub-account's transfers.
- [ ] Verified on production data against the base address's full list.
- [ ] **Docs updated** — `docs/architecture/**` for the new endpoint/view.
- [ ] **API types regenerated** if `crates/api/**` changes.
