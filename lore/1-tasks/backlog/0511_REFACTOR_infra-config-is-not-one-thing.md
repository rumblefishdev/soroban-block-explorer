---
id: '0511'
title: 'REFACTOR: the declared infrastructure configuration is not one thing'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0455']
tags:
  ['infrastructure', 'cdk', 'configuration', 'effort-medium', 'priority-medium']
links: []
history:
  - date: 2026-08-19
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0455 review sweep (findings 65, 67, 68). Three reports,
      one shape: the configuration that describes this infrastructure does not
      live in one place and nothing checks that its parts agree. Same class as
      0455's defect 1 — declared versus actual, never compared — but on the
      infrastructure side rather than the data side.
---

# REFACTOR: the declared infrastructure configuration is not one thing

## Summary

Three independent findings say the same thing from three angles. The values
that describe this deployment are spread across a single oversized object, a
process environment variable read at synth time, and seven files plus an
operator's shell — with nothing comparing any of it.

None is broken today. Each removes the ability to answer "what does this
deployment actually declare" by reading one thing.

## Context — all verified 2026-08-19

### A. One object, sixty-three fields, every stack depends on all of it

`infra/src/lib/types.ts` is 658 lines and carries a single configuration
interface with 63 `readonly` fields. Every stack takes the whole object.
Consequence: changing one field re-synthesises every stack, so the blast
radius of a one-line config edit is the entire application, and `cdk diff`
cannot distinguish "this stack is affected" from "this stack was re-rendered".

### B. The account comes from the environment at synth time

`infra/src/lib/app.ts:30` and the CI app entry take the AWS account from
`CDK_DEFAULT_ACCOUNT`, which is supplied by CI and not by the committed
configuration. A synth run on a laptop and a synth run in CI are therefore not
guaranteed to describe the same account, and nothing in the repository states
which account production is.

### C. One hostname, two halves, no comparison

`chDomainName` appears in seven files under `infra/`. The Ansible half of the
system takes the same host from an operator-exported variable. Two independent
declarations of one value, maintained by hand, with nothing that fails when
they diverge — the same shape as the CN-to-user map, where the repository's
documented example and the live environment variable have already drifted
(0455 finding 61).

## Implementation

The three are separable and can land in any order; they share a direction
rather than a sequence.

- **A** — split the config object along the boundaries the stacks already have,
  so a stack declares what it consumes. The 63 fields are not the problem; one
  undifferentiated object is.
- **B** — put the account in the committed environment config and fail synth if
  the environment disagrees, rather than reading the environment as the source.
- **C** — decide which side owns the hostname, derive the other from it, or add
  a check that fails when they differ. A comparison is enough; unification is
  not required.

## Acceptance Criteria

- [ ] A stack's dependency on configuration is narrower than "the whole object";
      changing an unrelated field does not re-synthesise it
- [ ] The production account is stated in committed configuration; a synth with
      a conflicting environment fails rather than silently retargeting
- [ ] The ClickHouse hostname has one owner, and a divergence between the CDK
      and Ansible halves fails something
- [ ] **Docs updated** — `docs/architecture/infrastructure/**` describes where
      configuration lives after the change
- [ ] **API types regenerated** — N/A, no API surface change
