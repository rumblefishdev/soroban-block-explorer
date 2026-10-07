---
id: '0630'
title: 'BUG: "Domain" means two things on assets — on-chain home_domain on the list, home_page hostname on the detail'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0450', '0469']
tags: [frontend, api, assets, phase-future, effort-small, priority-low]
links: []
history:
  - date: '2026-10-06'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0450 future work. 0450 shipped the issuer domain on the
      assets list; its "one meaning of Domain" and docs criteria had no home
      when it was archived.
---

# "Domain" means two things on assets

## Summary

The assets list shows the issuer's on-chain `home_domain`
(`issuer_home_domain`, task 0450). The asset detail page labels a `Domain` row
with the hostname of `home_page`, a SEP-1 value
(`web/src/pages/assets/AssetMetadata.tsx:43-46,80`). Same label, two sources:
they can disagree for the same asset.

## Implementation

- Pick one meaning for the `Domain` label on the asset detail page. The on-chain
  `home_domain` is already on the detail response (`issuer_home_domain`); the
  SEP-1 hostname can keep its own label (e.g. "Homepage") if it stays.
- Write the field into the assets endpoint contract under
  `docs/architecture/**` (ADR 0032) — the docs criterion 0450 left open.

## Acceptance Criteria

- [ ] Asset detail and assets list show the same value under "Domain"
- [ ] **Docs updated** — assets endpoint contract under `docs/architecture/**`
- [ ] **API types regenerated** — only if `crates/api/**` changes
