---
id: '0450'
title: 'FEATURE: show the issuer home domain in the assets-list issuer column (already fetched, dropped at serialisation)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0319', '0334', '0364', '0371']
tags: [backend, api, frontend, assets, priority-medium, effort-small]
links: []
history:
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Spawned from external feedback on the live deployment: when searching an
      asset by code, show the issuer name alongside the address in the issuer
      column — `G… - Centre.io`. The requested value is the issuer's
      **home domain**, not an organisation name, and the list path already
      fetches it for every row; it is discarded before serialisation. Separate
      feedback stream from the batch that produced 0440-0445.
---

# FEATURE: issuer home domain in the assets-list issuer column

## Summary

The assets list renders only the issuer StrKey under "Issuer / Contract ID".
Render the issuer's `home_domain` beside it (`GA5ZSE… · centre.io`). The value is
already read from ClickHouse on the list path and thrown away at the DTO
boundary, so this costs one response field and one cell change — no new query,
no new join, no extra read.

## Why it is nearly free

The list resolves its page's issuers through a bloom-pruned `accounts.id`
key-seek rather than a join (0319, 0334), and that seek **already selects
`home_domain`**:

- `crates/api/src/assets/queries.rs:290-304` — `seek_latest_account` selects
  `id, account_id, home_domain`, taking the newest row by `last_seen_ledger`
  because `home_domain` is mutable via `SET_OPTIONS`.
- `crates/api/src/assets/queries.rs:777-790` — the list page resolves all its
  issuers via `resolve_page_issuers`, run concurrently with hydration under one
  `tokio::join!` (0364).
- `crates/api/src/assets/queries.rs:254-277` — `list_row_to_asset_row` maps the
  result onto `AssetRow.issuer_home_domain`, for list rows and detail alike.

Then it stops. `issuer_home_domain` appears nowhere in
`crates/api/src/assets/dto.rs` and nowhere in `libs/api-types/src/openapi.json`;
its only consumer is `crates/api/src/assets/handlers.rs:262`, which uses it
internally to drive the SEP-1 lookup behind the detail page's `description`. The
frontend has never seen the field.

## Scope

1. Add `issuer_home_domain: Option<String>` to the assets **list** item DTO
   (the detail DTO may want it too — check before assuming).
2. Regenerate API types.
3. `web/src/pages/assets/AssetsTable.tsx:70` — render the domain as a secondary
   line under the StrKey in the existing "Issuer / Contract ID" cell. The StrKey
   stays the copyable canonical value.

## Constraints

- **Not an identity claim.** `home_domain` is set by the account holder and is
  unverified on its own — an issuer can set any domain. Render it as a claim,
  never as a badge implying we checked it. (SEP-1 `TOML` reachability would be
  weak corroboration at best; out of scope here.)
- **Sparse.** Most issuers set no `home_domain`; the cell must look deliberate
  when the value is absent, not broken.
- The branch that renders a contract StrKey (soroban / SAC facet) has no issuer
  and must be left alone.

## Acceptance criteria

- [ ] `issuer_home_domain` present on the assets-list item response
- [ ] No additional ClickHouse round trip vs today — verified by comparing the
      query count / `read_rows` on a list page before and after
- [ ] Column renders `StrKey` + domain; absent domain degrades cleanly
- [ ] Contract-backed rows (soroban, SAC facet) unchanged
- [ ] **Docs updated** — assets endpoint contract under `docs/architecture/**`
      per ADR 0032
- [ ] **API types regenerated** — touches `crates/api/**`; run
      `npx nx run @rumblefish/api-types:generate`

## Notes

0371 (asset search by project name / issuer domain) wants the same field as a
_search input_; this task only displays it. Landing this one first gives that
search a visible target to match against.
