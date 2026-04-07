---
id: '0107'
title: 'Fix API custom domain DNS resolution'
type: BUG
status: active
related_adr: []
related_tasks: ['0035']
tags: [priority-high, effort-small, layer-infra]
milestone: 1
links: []
history:
  - date: 2026-04-07
    status: backlog
    who: fmazur
    note: 'Discovered during task 0039 staging deploy verification.'
  - date: 2026-04-07
    status: active
    who: fmazur
    note: 'Activated task'
---

# Fix API custom domain DNS resolution

## Summary

`api.staging.sorobanscan.rumblefish.dev` does not resolve publicly despite Route 53 records existing. The API works under the auto-generated API Gateway URL but not under the custom domain.

## Context

Route 53 has two hosted zones:

- `sorobanscan.rumblefish.dev` (Z10396861CRMUIWWA8TL9)
- `staging.sorobanscan.rumblefish.dev` (Z0920117B9QSLT9FXPPW)

The A/AAAA record for `api.staging.sorobanscan.rumblefish.dev` exists in the parent zone (`sorobanscan.rumblefish.dev`), but public DNS returns NXDOMAIN. Likely cause: the record is in the wrong hosted zone, or NS delegation from parent to child zone is missing.

## Acceptance Criteria

- [ ] `curl https://api.staging.sorobanscan.rumblefish.dev/health` returns 200
- [ ] DNS resolves publicly (verified via `nslookup` against 8.8.8.8)
