---
id: '0211'
title: 'RESEARCH: Asset USD price exposure — design (Oskar price API consumer)'
type: RESEARCH
status: backlog
related_adr: ['0029', '0043']
related_tasks: ['0188', '0191', '0194', '0195', '0199']
tags:
  [
    priority-medium,
    effort-medium,
    layer-backend,
    layer-api,
    enrichment,
    blocked-on-oskar-api,
  ]
milestone: 2
links:
  - https://developers.stellar.org/docs/data/oracles/oracle-providers
history:
  - date: '2026-05-12'
    status: backlog
    who: karolkow
    note: >
      Spawned after 0197 audit-prep punch-list review. Closes a recurring
      planning loop: `assets.usd_price` was suggested 4 times across 0188 /
      0191 / 0194 §1a / 0195 §2c, pulled 4 times as YAGNI (no consumer / no
      shipped sort variant / no product ask). 0197-prep wave re-added it as
      0199 Phase 2b, which exposed the deeper question: a stored column
      cannot serve a value that changes per-second, so the design pattern
      itself was wrong. This task answers the design before any task ships
      the consumer. Blocked on Oskar's price API contract.
---

# RESEARCH: Asset USD price exposure — design (Oskar price API consumer)

## Summary

How does the block explorer expose per-asset USD price?

The team is building an internal price API (Oskar). This task is **not** about
building the price source — it is about how `crates/api` consumes that source
and surfaces price to:

- `GET /v1/assets/:id` detail page
- `GET /v1/assets` list (sortable by price? not sortable?)
- A future markets-style ranking view (separate endpoint? bolted onto list?)

The hard constraint that broke all four previous attempts: **USD price is
fast-change off-chain data**. ADR 0043's "off-chain + list endpoint → Lambda 2
column" rule assumes off-chain data is _rare-change per row_ (SEP-1 TOML, NFT
metadata, LP-at-snapshot-moment). USD price violates that assumption — every
stored value is stale within seconds of write. This task must decide whether
to bend the rule, add a fourth path, or sidestep with a dedicated endpoint.

## Context

### Why every previous attempt failed

| Attempt | Where                                                        | Outcome                                                                      |
| ------- | ------------------------------------------------------------ | ---------------------------------------------------------------------------- |
| 1       | 0188 Future Work (2026-05-05, brain-dump at task completion) | Listed as parking-lot bullet; no implementation                              |
| 2       | 0194 §1a (schema additions: column + 5 partial indexes)      | Pulled mid-task as YAGNI — "no shipped sort variant uses it"                 |
| 3       | 0195 §2c (`asset_usd_price` enrichment kind)                 | Dropped from M2 — "no consumer"                                              |
| 4       | 0199 Phase 2b (extend LP analytics scope to assets)          | Reverted same day — semantically wrong, stored value is stale within seconds |

Each pull cited a different surface-level reason, but the root cause is the
same: **no architectural pattern for "off-chain fast-change list-endpoint
data" exists in this codebase**. The four parked attempts deferred the
question; this RESEARCH task answers it before anyone tries a fifth.

### Audit-doc lineage

`docs/audits/2026-04-10-pipeline-data-audit.md` line 532 listed asset USD
price under "off-chain fields → SQS-driven type-1 worker Lambda". That
classification was correct as a _transport choice_ but does not address the
freshness problem. The 2026-05-12 audit-prep wave (this task spawn)
re-evaluates and forwards the design question to a dedicated research pass.

### Decision is gated on Oskar's API

The team-built price API (Oskar) is the only price source for the explorer.
Direct Reflector RPC integration, CoinGecko / StellarExpert / Horizon
aggregation, etc. all live behind Oskar's API. We therefore only design the
_consumer_ side; Oskar decides the upstream.

Required capability information from Oskar before this research can converge:

- Batched endpoint (multi-asset in 1 call)?
- Latency p50/p95?
- Rate limit per consumer?
- Server-side cache TTL?
- No-price-for-asset behavior (404 / empty / null)?
- Stale-value semantics for low-liquidity assets?

## What Would Answer This

The research is complete when we have all of:

- Q-note framing the design question concisely.
- R-note documenting Oskar's API capabilities (a spike based on real
  consultation, not assumption).
- R-note documenting how other Stellar explorers expose asset price
  (stellar.expert, stellarchain.io) — based on observable API behavior +
  any published docs.
- S-note (synthesis) picking one of the four candidate designs:

  | Design                            | `/v1/assets`                  | `/v1/assets/:id`   | `/v1/markets` (new) | Storage                                  |
  | --------------------------------- | ----------------------------- | ------------------ | ------------------- | ---------------------------------------- |
  | A — Stored column                 | sortable, stale 0-60s         | served from column | n/a                 | DB column + janitor refresh via Lambda 2 |
  | B — Runtime batched               | fresh, list = 1 batched fetch | runtime fetch      | n/a                 | none, app-side cache only                |
  | C — Markets endpoint              | no price                      | no price           | sortable, fresh     | none, separate handler                   |
  | D — Hybrid (B detail + C markets) | no price                      | runtime fetch      | sortable, fresh     | none                                     |

- G-note specifying:
  - Endpoint shapes (handler routes, DTO additions, cache TTLs).
  - Failure modes (Oskar 5xx, timeout, rate limit, no-price-for-asset).
  - Cache invalidation strategy.
  - ADR 0043 amendment draft (the rule needs a fourth path or an explicit
    exception for fast-change off-chain data; the amendment must be at
    _least_ sketched here even if the new ADR is its own task).

## Out of Scope

- Building the price API (Oskar's work).
- Picking upstream sources (CoinGecko / StellarExpert / Reflector — Oskar
  decides).
- Markets-view product specification (paginate semantics, top-N filters,
  trending, etc.) — separate product task triggered by S-note outcome.
- Asset-level price history / charts (different problem, separate task).

## Notes

- **Why blocked-on-oskar-api, not active.** Without Oskar's batched / latency
  / rate-limit contract, the S-note can't honestly pick between B/C/D.
  Promoting now would invent assumptions and re-introduce the same
  scope-creep pattern this task exists to break.
- **ADR 0043 amendment is part of the deliverable.** Pulling 4 previous
  attempts surfaced an ADR gap: the rule overreached when it assumed
  off-chain = rare-change. Whatever S-note chooses, the rule needs to
  acknowledge the fast-change off-chain category explicitly.
- **0199 stays out of this.** 0199 Phase 2b was reverted in commit
  `7f7ca42`. The LP analytics task does not depend on asset USD price
  exposure — LP TVL/volume/fee*revenue use price \_as a multiplier at
  snapshot moment*, not as a persisted asset attribute. The two concerns
  share Oskar's API but solve different problems.
