---
id: '0440'
title: 'FEATURE: LP list asset filter — substring + pair syntax (exact-match only today; explicitly not user regex)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0371']
tags:
  [
    backend,
    api,
    frontend,
    liquidity-pools,
    search,
    priority-medium,
    effort-small,
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/366'
history:
  - date: '2026-07-27'
    status: backlog
    who: karolkow
    note: >
      Spawned from external feedback on the live deployment ("wished someone
      implemented regex for search in pools"). Investigation found the filter is
      weaker than the reporter assumed — exact match, not substring — and that
      the input placeholder overpromises. Scoped to substring + pair syntax;
      user-supplied regex deliberately rejected (see Rejected below).
---

# FEATURE: LP list asset filter — substring + pair syntax

## Summary

The liquidity-pool list filter matches a whole asset code exactly, so `USD`
returns nothing for `USDC` pools, and there is no way to filter by a _pair_
despite the input inviting it. Add substring matching and a `A/B` pair syntax.

## Current behaviour

`crates/api/src/liquidity_pools/queries.rs:975-979`:

```
AND (upper(lp.asset_a_code) = ? OR upper(lp.asset_b_code) = ?)
```

Exact equality on an upper-cased code, one code per request. Consequences:

- `USD` does not match `USDC` — the input has no partial matching at all.
- Only one leg can be constrained; there is no pair filter.
- `web/src/pages/liquidity-pools/PoolsFilterBar.tsx:65` labels the field
  **"Filter by asset pair…"**, which the backend cannot honour. The placeholder
  is the immediate user-visible defect even if the query is left alone.

Distinct from the global search bar (task 0271, completed) — that is a separate
endpoint and does not back this filter.

## Scope

1. Substring match on either leg's code, anchored to a sane minimum length
   (2–3 chars) to keep the scan bounded.
2. Pair syntax: `USDC/XLM` constrains both legs, order-insensitive.
3. Fix the placeholder to describe what the field actually does.

## Rejected: user-supplied regex

The original request was for regex. Not shipping it on a public endpoint: an
arbitrary caller-supplied pattern is an unbounded-backtracking risk against the
read-only ClickHouse profile, on top of the existing read-row quota. Substring
plus pair syntax covers the realistic uses (`USD…`, `…BTC`, `USDC/XLM`) without
handing a query planner to anonymous callers. Record the reasoning in the reply
to the reporter, not just here.

## Acceptance criteria

- [ ] Substring match on `asset_a_code` / `asset_b_code`, min-length guarded
- [ ] `A/B` pair syntax, order-insensitive
- [ ] Placeholder text matches actual behaviour
- [ ] Query cost measured (`read_rows`) on the busiest codes; no regression vs
      the current exact-match plan
- [ ] Regex explicitly not accepted; malformed input rejected, not passed through
- [ ] **Docs updated** — endpoint filter semantics under
      `docs/architecture/**` if the LP endpoint contract is documented there
- [ ] **API types regenerated** — touches `crates/api/**`; run
      `npx nx run @rumblefish/api-types:generate`
