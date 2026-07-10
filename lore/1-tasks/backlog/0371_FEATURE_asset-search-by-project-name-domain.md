---
id: '0371'
title: 'FEATURE: asset search by project name / issuer domain (e.g. "Centrifuge" -> deJTRSY)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0370']
tags:
  [
    'backend',
    'api',
    'search',
    'assets',
    'enhancement',
    'effort-medium',
    'priority-low',
  ]
links: []
history:
  - date: 2026-07-10
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from 0370 future work. Low-priority enhancement.'
---

# FEATURE: asset search by project name / issuer domain

## Summary

Let users find assets by the _project / brand_ name or issuer domain, not just
the on-chain code/symbol. E.g. searching "Centrifuge" should surface `deJTRSY`,
"Circle" → USDC. Total enhancement, **priority low**.

## Context

Follow-up from **0370** (asset-list search by name/symbol). 0370 makes tokens
findable by their _on-chain_ name/symbol (`Solv`, `deJTRSY`), but not by the
project brand: a Soroban-native token's on-chain name is the token code
(`deJTRSY`), and it has no issuer account, hence no `home_domain`. So "Centrifuge"
matches nothing. Same Ada report (2026-07-10) that produced 0370.

## Implementation (sketch)

- Decide the alias source:
  - a curated alias/tag field on `asset_enrichment` (e.g. `project_name` /
    `tags`), populated by the enrichment worker or a small curated list; and/or
  - issuer `home_domain` (for classic assets that DO have an issuer) as an extra
    search dimension.
- Extend the asset search predicate (0370's clause) to also match the
  alias/domain column(s).
- Populate the curated data (backfill for the known RWA / brand assets).

## Acceptance Criteria

- [ ] Searching a known brand (e.g. "Centrifuge") returns its asset(s).
- [ ] Curated alias data has a defined write path (not a one-off hardcode).
- [ ] **Docs updated** — update the search/enrichment description in
      `docs/architecture/**` if a new column/table is added.
- [ ] **API types regenerated** — if the DTO/response changes under
      `crates/api/**` or `libs/api-types/**`.

## Notes

Priority low — "total enhancement". Depends on 0370's search predicate.
