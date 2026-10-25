---
id: '0644'
title: 'Delete the three dead `infra:*:staging` scripts'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0249', '0390']
tags: [tooling, priority-low, effort-small]
history:
  - date: '2026-10-15'
    status: backlog
    who: karolkow
    note: >
      Found while auditing the root CLAUDE.md: it and docs/deployment.md both
      claimed the staging scripts do not exist. They do — only the Makefile
      targets behind them are gone. Filed as 0643 and renumbered the same day:
      another session had already used 0643 in its commit messages.
---

# Delete the three dead `infra:*:staging` scripts

## Summary

`package.json` defines `infra:diff:staging`, `infra:deploy:staging` and
`infra:synth:staging` (lines 19, 21, 23). Each calls a `make -C infra
*-staging` target, and `infra/Makefile` has none (0 matches for "staging"), so
each script errors immediately. Staging was retired by task 0249 and its
leftovers removed by task 0390; these three were missed.

## Implementation

- [ ] Remove the three scripts from `package.json`.
- [ ] Drop the now-redundant sentence in `docs/deployment.md` § No staging and
      in the root `CLAUDE.md` guides table (both currently explain that the
      scripts exist and fail).
- [ ] Grep for callers first: a CI workflow or a shell alias naming them.

## Acceptance Criteria

- [ ] `grep -c staging package.json` is 0.
- [ ] `docs/deployment.md` § No staging reads as "nothing named staging exists".
