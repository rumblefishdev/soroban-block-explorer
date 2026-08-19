---
id: '0129'
title: 'D3: Stellar team read-only IAM access'
type: FEATURE
status: done
related_adr: []
related_tasks: ['0036', '0040']
tags: [priority-low, effort-small, layer-infra, audit-gap]
milestone: 3
links:
  - docs/audits/2026-04-10-pipeline-data-audit.md
history:
  - date: '2026-04-10'
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from pipeline audit — D3 acceptance criteria #3 requires Stellar team monitoring access but no task existed.'
  - date: '2026-08-19'
    status: done
    who: karolkow
    note: >
      Closed without building the role. Access is granted on request, which is what
      the Milestone 3 evidence package states; no standing cross-account IAM role is
      provisioned and none is planned. Reopen only if a request is actually made.
---

# D3: Stellar team read-only IAM access

## Summary

Deliverable 3 acceptance criteria #3 requires providing the Stellar team with read-only
access to production monitoring dashboards (CloudWatch).

## Implementation

1. Create IAM role with read-only CloudWatch access.
2. Configure cross-account access or shared dashboard link.
3. Document access instructions for Stellar team.

## Acceptance Criteria

- [ ] IAM role with read-only CloudWatch access created — not built, on request
- [ ] Stellar team can view production dashboards and alarms — on request
- [x] Access documented — `docs/scf/milestone-3-evidence.md` § 3 states read-only
      access is available on request

## Outcome

Closed without implementation. No standing read-only IAM role exists — the CDK
sources under `infra/src` define no such role (the only `stellar*` hit is
`stellarNetworkPassphrase` in `types.ts`), and none is planned. Live IAM was not
queried; this is a source-level check. Access will be provisioned if and when the
Stellar team asks for it.

What the Milestone 3 package says today: `docs/scf/milestone-3-evidence.md` § 3
(Live Endpoints and Reviewer Access) — "Read-only access for the Stellar team is
available on request."

**Known gap in the evidence wording.** AC3 is quoted verbatim in the package as
"CloudWatch dashboard accessible to Stellar team (read-only IAM role)". The
§ Scope Refinement section lists three deviations from the approved plan — the
p95 miss, the RDS-specific data-at-rest wording, and the one raised alarm — but
not this one. On-request access instead of a standing role is therefore a fourth
deviation that the package does not declare. Worth adding a Scope Refinement
point if the package is revised; tracked here rather than left unrecorded.
