---
id: '0534'
title: 'BUG: asset search returns impostors and cannot parse CODE:ISSUER'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0331', '0370', '0470']
tags: [backend, api, search, assets, priority-high, effort-small]
links: []
history:
  - date: 2026-09-03
    status: backlog
    who: stkrolikiewicz
    note: 'Task created. Both defects reproduced against production ClickHouse.'
---

# BUG: asset search returns impostors and cannot parse CODE:ISSUER

## Summary

Two defects in `/v1/search`, both found while checking how USDT0 is indexed.
They compound: the asset bucket ranks by nothing, and the one query shape that
would disambiguate an asset exactly — the canonical `CODE:ISSUER` — is not
recognised at all. **Searching `USDC` today returns ten assets, none of which is
Circle's USDC.**

## Context

### Defect 1 — the asset bucket has no ORDER BY

[`search_assets`](../../../crates/api/src/search/queries.rs) (phase 1, around
line 743) is `positionCaseInsensitive(asset_code, q) > 0 … LIMIT
{per_group_limit}` with **no ordering**. The default limit is 10
(`handlers.rs`, `DEFAULT_LIMIT`), so ten arbitrary rows in ClickHouse scan
order win.

Reproduced on production 2026-09-03 — 468 assets carry a code containing
`USDC`; the ten the query returns:

| asset_code   | issuer    | holders |
| ------------ | --------- | ------- |
| BUSDC        | GBTUV7KU… | 0       |
| Binance2USDC | GBCSGTWR… | 21      |
| BitstampUSDC | GBG4ARJR… | 0       |
| ExtraUSDC    | GDDUEKGH… | 1       |
| yUSDC        | GDGTVWSM… | 10,558  |
| …            |           |         |

Circle's `USDC-GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN`
(**691,406 holders**) is not in the result set. The most widely held asset on
the network is unreachable through search by its own code.

USDT0 is the same shape with a sharper edge: four issuers share the code, and
sorting by supply puts an impostor first —

| issuer                        | raw supply             | holders |
| ----------------------------- | ---------------------- | ------- |
| GA7GNGYV…                     | 30,000,000,000,000,000 | 1       |
| **GATISXX… (real, usdt0.to)** | 25,950,252,376,996     | 92      |
| GAKSY7RQ…                     | 99,999,974,542,081     | 2       |
| GDBDGR2U…                     | 99,997,539,800         | 1       |

**`home_domain` is not the signal.** All four USDT0 issuers read NULL in
`accounts`, and Horizon confirms the real one genuinely never set it — this is
not ingestion drift. `holder_count` is the discriminating field, and
`balance_aggregates` (task 0331) already has it precomputed.

### Defect 2 — `CODE:ISSUER` classifies as nothing

`USDT0:GATISXX6BZ6NC7IKQBY37CJD4SOZL3CYZJWXEDG6JVIY4WBS6KXJHN6Q` is the
canonical asset form in the SDKs and SEPs, and it is the most precise thing a
user can type. Today:

- [`classify`](../../../crates/api/src/search/classifier.rs) returns
  `Classified::default()` — not 64-hex, not an L-strkey, does not start with
  `G`/`C`, so both fields stay `None`.
- The asset bucket then runs a 63-character needle against a ≤12-character
  `asset_code`: **provably zero rows**.
- The account bucket never fires either — the string does not begin with `G`.

Net effect: the most precise query returns an empty page, while the vaguest one
returns impostors.

## Implementation Plan

### Step 1 — rank the asset bucket

Add a `LEFT JOIN balance_aggregates` (1:1 on `assets.id`, the same join the
assets list already uses) and order exact-code matches first, then by holders:

```sql
ORDER BY (lower(toString(a.asset_code)) = lower(?)) DESC,
         b.holder_count DESC NULLS LAST
```

Verified against production before writing this task — with the clause, Circle's
USDC is row 1 with a 200× holder lead over row 2.

Deliberately **not** ranking by supply: supply is the field the impostors
inflate (30 quadrillion, one holder). Holders cost real trustlines.

### Step 2 — classify `CODE:ISSUER`

Split on the last `:` or `-`; if the right-hand side is a full G-strkey and the
left is 1–12 alphanumerics, carry both in a new `Classified` field. Stellar
asset codes are `[a-zA-Z0-9]` only, so neither separator can appear in a code
and the split is unambiguous. Accept `-` as well as `:` — that is the shape our
own `/assets/:id` routes emit, so users will paste it back.

`search_assets` then does an exact `asset_code = ? AND issuer = ?` lookup
instead of the substring scan.

### Step 3 — tests

`classifier.rs` already has a table-driven test module — extend it: both
separators, lowercase code, a bad checksum on the issuer, a code with a
separator-shaped neighbour. One integration-level assertion that ranking puts
the highest-holder asset first.

## Acceptance Criteria

- [ ] Searching `USDC` returns Circle's
      `GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN` first
- [ ] Searching `USDT0` returns `GATISXX…` above the three impostors
- [ ] `USDT0:GATISXX…` and `USDT0-GATISXX…` both resolve to exactly one asset hit
- [ ] A `CODE:ISSUER` whose issuer fails checksum degrades to today's behaviour,
      not a 500
- [ ] **Docs updated** — `docs/architecture/database-schema/endpoint-queries-clickhouse/22_get_search.sql`
      carries the new ORDER BY and the `CODE:ISSUER` branch. Other
      `docs/architecture/**` N/A: no new table, endpoint or pipeline step.
- [ ] **API types regenerated** — required (touches `crates/api/**`);
      `npx nx run @rumblefish/api-types:generate` in the same commit if the
      response shape moves. If only SQL changes, the diff is empty — say so
      rather than skipping the run.

## Notes

Holder count is refreshed by `balance_aggregates_mv` every 2 minutes, so the
ranking is eventually consistent. That is the right trade — a two-minute-stale
holder count still separates 691,406 from 0.

The join adds one small-table lookup to a bucket that already does a `FINAL`
scan of `assets`; measure before assuming it is free, but 146k rows against a
1:1 aggregate is not the expensive half of this query.

Adjacent, not covered here: [0370](../archive/) made type-3 assets findable by
metadata name. This task does not touch name matching — an impostor that copies
the _name_ rather than the code is a separate problem, and holder ranking
happens to blunt it too.

Out of scope: a verified-issuer overlay (curated list, SEP-1 TOML, or a
directory feed). Worth its own task if impersonation turns into a support load;
holder ranking is the cheap 90% and needs no new data source.
