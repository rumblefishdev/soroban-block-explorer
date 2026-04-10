---
id: '0128'
title: 'D3: public GitHub repository setup'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0127', '0129']
tags: [priority-low, effort-small, layer-ops, audit-gap]
milestone: 3
links:
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-04-10'
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from pipeline audit — D3 acceptance criteria #2 requires public repo but no task existed.'
---

# D3: public GitHub repository setup

## Summary

Deliverable 3 acceptance criteria #2 requires the repository to be made public. This
involves reviewing the codebase for secrets, sensitive configuration, and proprietary
content before making the repo public.

## Known Sensitive Data in Git History

The pipeline audit (2026-04-10) identified:

- AWS Account ID `750702271865` in `infra/envs/staging.json` and a worklog file
- Full ACM certificate ARN in `infra/envs/staging.json:41`

These must be scrubbed using `git-filter-repo` or `BFG Repo Cleaner` before making the
repo public. Consider moving staging config values to SSM Parameter Store.

## Implementation

1. Audit git history for secrets using `trufflehog` or `gitleaks`.
2. Scrub identified sensitive data using `git-filter-repo`.
3. Move environment-specific values to SSM Parameter Store / Secrets Manager.
4. Add LICENSE file and update README.md.
5. Change repository visibility to public.

## Acceptance Criteria

- [ ] Git history scanned with `trufflehog`/`gitleaks` — zero findings
- [ ] AWS Account ID and certificate ARN scrubbed from history
- [ ] Environment config values moved out of committed files
- [ ] LICENSE file present with appropriate open-source license
- [ ] README.md with project description, setup instructions, contribution guidelines
- [ ] Repository visibility changed to public
