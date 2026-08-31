---
id: '0526'
title: 'Split god files incrementally — by topic, tests always extracted'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0374']
tags: ['tooling', 'code-health', 'effort-medium', 'incremental']
links: []
history:
  - date: '2026-08-31'
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0374 simplify round: the repo review confirmed the
      god-file pattern (files grow per LAYER — one queries.rs per module —
      instead of per topic). Methodology recorded in the repo CLAUDE.md
      (decision 18C, no CI gate); this task owns working the existing
      backlog down. 0374 already extracted the LP module's tests as the
      pattern to follow.
---

# REFACTOR: split god files incrementally — by topic, tests always extracted

## Summary

Largest source files hold whole layers and their tests inline, which makes
review, navigation and targeted testing slow. Shrink them RATAMI — a file is
split (or at minimum has its tests extracted) when a PR touches it — never as
a big-bang refactor. The rule lives in the repo `CLAUDE.md` ("File Size &
Test Placement"); this task tracks the backlog of existing offenders.

## The measured offenders (2026-08-31, `wc -l`, tests inline unless noted)

| file                                                   | lines | note                                                                                                                     |
| ------------------------------------------------------ | ----- | ------------------------------------------------------------------------------------------------------------------------ |
| `crates/api/src/liquidity_pools/queries.rs`            | 3,140 | tests already extracted (0374); still one file for list+detail+chart+activity+participants SQL — split by endpoint topic |
| `crates/xdr-parser/src/state.rs`                       | 3,427 |                                                                                                                          |
| `crates/db-clickhouse/src/persist/tests_cross.rs`      | 3,301 | already a test file; split by table family                                                                               |
| `crates/db-clickhouse/src/persist/stage.rs`            | 3,093 |                                                                                                                          |
| `crates/xdr-parser/src/operation.rs`                   | 1,633 |                                                                                                                          |
| `crates/xdr-parser/src/invocation.rs`                  | 1,580 |                                                                                                                          |
| `crates/enrichment-shared/src/nft_token_uri/client.rs` | 1,572 |                                                                                                                          |
| `crates/api/src/assets/queries.rs`                     | 1,362 |                                                                                                                          |
| `crates/api/src/liquidity_pools/handlers.rs`           | 1,244 | tests extracted (0374)                                                                                                   |
| `crates/api/src/contracts/queries.rs`                  | 1,245 |                                                                                                                          |

TS side is healthier (max ~1,000, tests already sibling files) — in scope
only if a file crosses the limit.

## Method (the CLAUDE.md rule, applied)

- Split BY TOPIC (one concern per file), never by layer — "all queries of
  the module" is how these grew.
- Tests always to a sibling file: Rust `foo_tests.rs` via
  `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;` (0374's LP extraction
  is the worked example); verification-only code (oracles) goes to
  `tests/` entirely (`share_token_real_corpus.rs` pattern).
- A split is its own `refactor(...)` commit: pure moves, zero behaviour
  change, so review is `git diff --color-moved`.
- Touching an offender in a feature PR? Extract at least its tests in that
  PR; the topic split can follow separately.

## Acceptance criteria

- [ ] Every file above either under ~800 production lines or split by topic
- [ ] No production module holds an inline `#[cfg(test)]` test module in the
      touched set
- [ ] Rule text in repo `CLAUDE.md` still matches practice (update it if the
      method evolves here)
