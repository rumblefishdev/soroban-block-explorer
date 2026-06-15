---
id: '0274'
title: 'Backend: API endpoints + fields surfaced by FE gaps audit'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0199', '0226', '0247', '0279']
tags:
  [
    priority-medium,
    effort-medium,
    layer-api,
    layer-backend,
    phase-pre-launch,
    milestone-2,
  ]
milestone: 2
links:
  - docs/audits/2026-05-29-frontend-api-gaps.md
  - tools/dev-mock-api.mjs
history:
  - date: '2026-05-29'
    status: backlog
    who: FilipDz
    note: >
      Spawned from the FE→API gaps audit
      (`docs/audits/2026-05-29-frontend-api-gaps.md`). FE built the
      Accounts page against an in-memory mock + worked around several
      missing fields on existing endpoints. This task tracks the
      backend work to close those gaps.
  - date: '2026-06-01'
    status: active
    who: karolkow
    note: >
      Activated alongside 0275 — taking both over as a pair. Prior
      WIP exists on `origin/feat/0274_backend-api-gaps-from-fe-audit`
      (FilipD, commit f0ff1a72): OpenAPI cleanups subset covering 3 of
      7 gaps (#3 ledgers `order` param, #6 `interface_metadata` schema
      + FE defensive-parser deletion, #7 `recent_events` on
      ContractStats). Remaining: #1 `GET /v1/accounts` (headline), #2
      lp_op_details amounts, #5 `PoolAssetLeg.icon_url`. Continuing on
      FilipD's branch (rebased on develop) rather than a fresh start to
      avoid duplicating his 3 gaps; branch to be renamed to span both
      0274 + 0275.
  - date: '2026-06-03'
    status: done
    who: karolkow
    note: >
      Closed. Headline #1 `GET /v1/accounts` shipped (`7021592d`):
      PG-only list endpoint + page, mirrors the assets/ledgers list
      pattern, swaps the FE off the in-memory mock. #5 `PoolAssetLeg.icon_url`
      (`8af433d9`) and #3/#6/#7 (FilipD `c6bec5ee` + correctness rework
      `08279072`) done. #2 `lp_op_details` deferred — deep-dive confirmed
      per-op LP amounts are genuinely not in the DB
      (`operations_appearances.amount` = fold count), `xdr_parser` has no
      LP-op extraction, and the LP-tx endpoint is DB-only (no archive
      scaffolding) — a real feature, not a quick add. Path decision →
      research 0247; implementation → spawned 0279. #4 (pool chart nulls)
      was never a 0274 AC — owned by 0199 (blocked-on-oracle). Accounts
      response intentionally diverges from the audit's draft shape (see
      Design Decisions § Emerged). 6 backend + 6 FE integration/unit tests
      added; api-types regen; docs/backend-overview §6.2/§6.3 updated.
---

# Backend: API endpoints + fields surfaced by FE gaps audit

## Summary

Implement the backend pieces FE called out in the
[FE→API gaps audit](../../../docs/audits/2026-05-29-frontend-api-gaps.md):
one new list endpoint, one expanded list, and four schema /
contract additions. Closes the FE workarounds (in-memory account
mocks, hidden Amount column on pool tx, hand-rolled
`interface_metadata` parser, etc.).

## Context

The FE is in design-parity / pre-launch state. The audit catalogues
seven concrete gaps blocking either a feature surface or a quality
detail:

> **Progress (2026-06-01).** Gaps #3, #6, #7 landed in `c6bec5ee` (FilipD's
> WIP rebased). A skeptical re-audit (existence ≠ correct) found two of the
> three were only _present_, not _correct_ — both hardened in `08279072`:
>
> - **#3 was broken** — asc reused `Direction::Prev`, presented oldest-first
>   block in DESC order + broke forward pagination. Reworked properly
>   (sticky `?order=` sort ⊥ cursor nav) + DB-backed behaviour test.
> - **#6 silently degraded** — unparseable metadata → `null` (warn-only).
>   Now fails loud (HTTP 500 + `interface_metadata_corrupt`).
> - **#7 verified genuinely correct** (`amount` = event count per
>   migration `0004:12`; `SUM` = total events).
>
> Remaining: #1, #2, #5. #4 stays with task 0199.

1. **No `GET /v1/accounts` list endpoint** — Accounts page renders
   from 80 in-memory synthesized rows (`useAccountsList.ts`).
2. **Per-op LP amounts missing** on
   `GET /v1/liquidity-pools/{pool_id}/transactions` — the "Amount"
   column in the pool-tx table is intentionally hidden.
3. ✅ **DONE — `order` query param** on `GET /v1/ledgers`. Was silently
   _ignored_ by the real backend (only the mock honoured it). Wired in
   `c6bec5ee` but with a broken asc (reversed order + dead forward
   pagination); reworked correctly in `08279072` — sticky `?order=` sort
   orthogonal to cursor navigation, `keyset_sql` 2×2 matrix, DB-backed
   behaviour test.
4. **Pool chart values always `null`** — endpoint contract is in
   the spec but `tvl` / `volume` / `fee_revenue` are `null` for
   every bucket until task **0199** (LP analytics + price oracle)
   ships. FE renders a placeholder card.
5. **`PoolAssetLeg.icon_url` missing** — pool avatars fall back to
   the first letter of the asset code instead of a real icon.
6. ✅ **DONE — `interface_metadata` typed schema** on
   `GET /v1/contracts/{contract_id}/interface` — typed DTO + OpenAPI
   schema; FE defensive parser deleted (`c6bec5ee`). Decode failure
   hardened in `08279072`: a present-but-unparseable blob now returns
   HTTP 500 + `interface_metadata_corrupt` instead of silently `null`.
   **Caveats:** (a) legacy-shape rows (e.g. `functions` as bare strings,
   no `wasm_byte_len`) now 500 until re-indexed — re-index before deploy;
   (b) end-to-end parse-success on freshly-indexed data not yet verified
   (only the indexer-output ↔ DTO shapes were read and confirmed to
   match; no fresh-indexed DB available locally to prove `Some`).
7. ✅ **DONE — real events count** (`recent_events`) on
   `ContractStats` — Events tab pill no longer borrows
   `recent_unique_callers` (`c6bec5ee`).

The audit doc has the full TypeScript shapes FE expects — it is now
the single reference for those shapes. (The runnable dev mock
`tools/dev-mock-api.mjs` was removed 2026-06-01, ahead of the AC
below — shapes live in the audit doc.)

## Implementation

Remaining gaps (#1, #2, #5) — each independent, can ship piecemeal:

- **`GET /v1/accounts` (list)** — Query: `limit`, `cursor`,
  `sort=xlm_desc|last_seen_desc|first_seen_desc`, `filter[q]`,
  `filter[with_domain]`. Response item:
  `{ account_id, xlm_balance, xlm_supply_percent, first_seen_ledger,
last_seen_ledger, home_domain, rank? }`. The `accounts` module already
  has detail + tx-list scaffolding to reuse (`crates/api/src/accounts/`).

  > **⚠ Resolve on paper before coding — schema can't fully back this shape:**
  >
  > - **`xlm_supply_percent` has no backing data.** No network-wide XLM total
  >   supply is stored (`assets.total_supply` is per-asset only). Mock fakes a
  >   constant. Decide: hardcode / `SUM(balance)` aggregate / drop for v1.
  > - **`xlm_balance` + `sort=xlm_desc` cross a table boundary** —
  >   balance is in `account_balances_current` (asset_type=0), not `accounts`.
  >   Cross-table keyset cursor + a new balance index needed.
  > - **`sort=first_seen_desc`** needs a new index (only `last_seen` exists).
  > - **`rank`** stable only for one sort mode, breaks under filter — design it.

- **`?expand=lp_op_details`** on pool transactions — opt-in field
  per row: `lp_operation_detail { operation_type, amount_a,
amount_b }`. Backend research tracked as **0247**; FE follow-up task
  TBD (the original "0249" cite was wrong — 0249 = archived AWS-teardown).
- **`icon_url` on `PoolAssetLeg`** — NOT a column copy: `PoolAssetLeg`
  carries only XDR `(code, issuer)`; `icon_url` lives on the `assets`
  row → LEFT JOIN per leg (2/pool). Design for the N+1 cost on the pool
  **list** endpoint.

Done: #3 `order` on `/v1/ledgers` (`c6bec5ee` + correctness rework
`08279072`), #6 `interface_metadata` schema (`c6bec5ee` + loud-fail
`08279072`), #7 `recent_events` on `ContractStats` (`c6bec5ee`, verified).
Pool chart fields (gap #4) are covered by **0199**.

## Acceptance Criteria

- [x] `GET /v1/accounts` ships behind the documented contract;
      FE swaps `useAccountsList` from in-memory mock to the generated
      SDK hook. (`7021592d`; shape diverges from the audit draft — see
      Design Decisions § Emerged.)
- [ ] `?expand=lp_op_details` on pool transactions is wired and the
      "Amount" column on the LP tx table is un-hidden FE-side.
      **Deferred** — path decision tracked in research **0247**,
      implementation spawned as **0279**. (The earlier "0249" cite was
      wrong — 0249 = archived AWS-teardown.)
- [x] OpenAPI declares the `order` param on `/v1/ledgers`, and asc
      actually returns oldest-first with working forward pagination.
      (`c6bec5ee` wired it; `08279072` fixed the asc semantics + test)
- [x] `PoolAssetLeg` carries `icon_url`; pool avatars render real
      icons when available. (`8af433d9`)
- [x] `InterfaceResponse.interface_metadata` has a real schema in
      OpenAPI; FE deletes `parseInterfaceMetadata`'s defensive parse.
      Decode failure surfaces as 500, not silent null. (`c6bec5ee` +
      `08279072`)
- [x] `ContractStats` exposes a real events count; FE points the
      Events tab pill at the new field. (`c6bec5ee`)
- [x] `tools/dev-mock-api.mjs` removed (done 2026-06-01, ahead of
      sequence — shapes preserved in the audit doc). FE still needs to
      point `VITE_API_BASE_URL` at the real backend once #1 ships.

## Notes

- The audit doc is the single source of truth for FE-side expected
  shapes — keep it in sync if backend semantics diverge during
  implementation.
- Mock-server divergences (transactions missing `contract_ids`,
  NFTs ignoring `filter[name]`, search shape mismatch) are
  intentionally **out of scope** here — they're FE dev-mock bugs,
  not real-API blockers.

## Implementation Notes (#1 `GET /v1/accounts`, 2026-06-03)

- **Backend** (`crates/api/src/accounts/`, `7021592d`): `AccountsListParams`
  (`order`, `filter[with_domain]`) + `AccountListItem`; `AccountsListCursor`
  `(last_seen_ledger, id)` keyset via `keyset_sql`; `fetch_list` joins native
  balance (`LEFT JOIN account_balances_current … asset_type=0`, `uidx_abc_native`);
  `list_accounts` handler (`Pagination` + `finalize_page` + `into_envelope`),
  route, openapi schema. PG-only, **zero migration**.
- **FE**: mock `useAccountsList` replaced by generated hook; `AccountsListPage`
  now mirrors the other list pages exactly; home_domain as inline chip;
  ledgers via `IdentifierDisplay type="ledger"`; sort on the Last Seen column.
- **Tests**: 6 backend integration (envelope + cut-field absence, cursor
  next + prev round-trips, order=asc, with_domain) + 6 FE unit. api-types regen.
- **Docs**: `backend-overview.md` §6.2 inventory + §6.3 prose for both
  `/accounts` and `/contracts`. (Canonical endpoint-queries SQL for the two
  list endpoints intentionally NOT added — see Issues.)

## Design Decisions

### Emerged

1. **Accounts response cut down vs the audit draft.** Dropped
   `xlm_supply_percent` (no network XLM-supply source — a constant would
   lie), the `#` `rank` column (keyset can't give a cheap global rank;
   it's also the only list with one — outlier), and address search
   (`filter[q]` / `sort=xlm_desc`/`first_seen_desc`). Rationale: StrKeys are
   opaque so a prefix filter is useless and exact lookup is the global-search
   redirect; balance/first_seen sort needs a new index (deferred). Final shape:
   `{ account_id, xlm_balance, last_seen_ledger, first_seen_ledger, home_domain }`,
   sort = `last_seen` only (the sole indexed dimension), filter = `with_domain`.
2. **`home_domain` as an inline chip** in the Account cell (not its own
   column — ~99% empty otherwise).
3. **Ledger references unified on `IdentifierDisplay type="ledger"`** —
   retrofitted `ContractsTable` ("Deployed at ledger") and `PoolParticipants`
   ("Since ledger") which had ad-hoc `<Link>` reinvents (`refactor(ui)` commit).
4. **clear-filters + URL-sort refactor** (separate commit `4dd716be`): a
   `useTableUrlState.clearFilters` primitive (sequential `setFilter(null)`
   clobbered under react-router's functional setter) + sort moved into the URL
   (`?sort=&dir=` via `setSort`) across all sortable list pages.

## Issues Encountered

- **#2 premise verified, not assumed.** Deep-dived whether per-op LP amounts
  could be served now without 0247: no — `operations_appearances.amount` is a
  fold count (ADR 0029), `xdr_parser` has no claimedOffers/deposit/withdraw
  extraction, the LP-tx endpoint is DB-only, and reserve-delta is unreliable
  (multi-op/ledger netting). Genuinely a feature → deferred.
- **Canonical SQL docs (24/25) reverted.** `24_get_accounts_list.sql` +
  `25_get_contracts_list.sql` + their README rows were added then removed
  (user decision); `backend-overview` §6.2/§6.3 carry the endpoint docs
  instead. The "one script per endpoint" convention is knowingly not met for
  these two list endpoints.
- **commit split slipped.** The intended 2-commit split (accounts vs ledger
  refactor) folded into one (`7021592d`) because lint-staged restored the
  unstaged ledger-fix files into the commit; left as-is per no-amend.

## Future Work

- **0279** — implement `?expand=lp_op_details` (#2) once 0247 picks the path.
- **0247** — research: XDR read-time fetch vs ingest-side extraction for LP amounts.
- **0199** — pool chart `tvl`/`volume`/`fee_revenue` (#4), blocked on price oracle.
