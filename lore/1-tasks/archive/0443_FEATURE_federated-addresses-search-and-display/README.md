---
id: '0443'
title: 'FEATURE: SEP-2 federated addresses — resolve name*domain in search (A), show it on accounts (B)'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0188']
tags:
  [backend, enrichment, frontend, accounts, sep2, priority-low, effort-medium]
links:
  - 'SEP-2 (Federation Protocol): https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0002.md'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/363'
history:
  - date: '2026-07-27'
    status: backlog
    who: karolkow
    note: >
      Spawned from external feedback on the live deployment: "if there's a
      federated address tied to this source account, it would be great to show
      it". Not implemented anywhere today — no prior task covers SEP-2.
  - date: '2026-09-01'
    status: active
    who: karolkow
    note: >
      Split into two independent scopes after a second external comment asked
      for the OTHER direction — typing `name*domain` into search. That
      direction (A) is browser-only, needs no backend and no storage, so it
      ships first; the original account-page direction (B) keeps the whole
      enrichment + SSRF cost and stays behind it. Activated for A.
  - date: '2026-09-02'
    status: active
    who: karolkow
    note: >
      Read SEP-1 and SEP-2 at the source and checked the implementation
      against them. Four corrections: internationalized domains were not
      classified at all; `>` was accepted in a username the spec excludes;
      federation answers were cached for five minutes against an explicit
      "should not be cached"; and the response-size cap two acceptance
      criteria claimed did not exist. Also gated the reverse direction on the
      domain shape — 7484 accounts carry a dotless `home_domain` (`Bankless`,
      `Indonesia`, `localhost:4000`, `1`, a bare space), every one of which
      was dialled before.
  - date: '2026-09-03'
    status: active
    who: karolkow
    note: >
      Measured the tail that the first CORS pass had left as "unknown". No
      violators in it at all: of 115 further domains (59450 accounts), 98.7%
      run no federation server, so they are not blocked, they simply have no
      answer. That drops the real cost of the CORS rule from an implied 16% of
      accounts to 1.9%, and settles the proxy question - not worth an SSRF
      surface in production.
  - date: '2026-09-02'
    status: active
    who: karolkow
    note: >
      B rewritten from scratch on the search branch instead of shipping
      separately: the same two hops already existed for A, so the reverse
      direction is one resolver, one hook and one summary row. Live sample of
      8 random accounts carrying `home_domain = lobstr.co`: 7 resolved to a
      name, so the row is worth showing. The transaction source-account
      surface followed the same day: the detail query already seeks the
      accounts row, so `home_domain` came along as one more column rather
      than the separate API task it looked like.
  - date: '2026-09-30'
    status: completed
    who: karolkow
    note: >
      Shipped 2026-09-02..04, live since production-2026.09.07-1. Every criterion met or declined; closed after a check 2026-09-30.
---

# FEATURE: SEP-2 federated addresses, both directions

## Summary

Search resolves a `name*domain` federated address to its account (scope A);
the account page shows the account's federated address (scope B).

## Acceptance Criteria

All met or declined (one declined: re-validating every redirect target) — the
per-scope checklists are in [notes/R-task-record.md](notes/R-task-record.md).

## Closing check (2026-09-30, read-only)

- The deployed SPA bundle carries the resolver (`stellar.toml`,
  `FEDERATION_SERVER`).
- Coverage recorded in the task: 83.5% of accounts with a home domain
  resolve; 1.9% are lost to servers without CORS.
