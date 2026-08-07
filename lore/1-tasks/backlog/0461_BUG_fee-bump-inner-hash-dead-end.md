---
id: '0461'
title: 'BUG: fee-bump inner hash is a dead end — not routable, not searchable'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0453', '0430']
tags:
  [
    backend,
    indexer,
    frontend,
    transaction-detail,
    search,
    priority-medium,
    effort-medium,
  ]
links: []
history:
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Reported by Karol while clicking through the shipped 0453 card: the
      Summary shows "Inner transaction cb69…42d3" as copy-only text — the
      link was deliberately withheld because /transactions/{innerHash} 404s
      (inner hashes are not indexed; 0359 spawn-plan #4, K3-2), and search
      does not resolve them either. A hash you can only copy into a search
      box that then finds nothing is worse than useless.
---

# BUG: fee-bump inner hash is a dead end

## Summary

Fee-bump wrappers carry `inner_tx_hash`; the detail page shows it (0453)
but nothing in the product can resolve it: the tx route looks up only outer
hashes and search does the same. Reporters and users landing on the inner
hash from wallets/Horizon links get a 404.

## Scope

- Indexer/DB: make the inner hash resolvable — either index it as an alias
  column on `transactions` (lookup by outer OR inner) or a small
  `inner_hash -> tx_id` mapping written at ingest; historical backfill for
  existing fee-bumps.
- API: `GET /v1/transactions/{hash}` resolves inner hashes to the wrapper
  transaction (redirect or transparent serve).
- Frontend: turn the Summary "Inner transaction" value into a real link;
  search resolves inner hashes.

## Acceptance criteria

- [ ] Opening /transactions/{innerHash} shows the wrapper transaction
- [ ] Search finds a transaction by its inner hash
- [ ] Summary "Inner transaction" is a working link
- [ ] Historical fee-bumps covered (backfill), not just new ingest
