---
id: '0074'
title: 'Frontend: Assets list and detail pages'
type: FEATURE
status: completed
related_adr: ['0036']
related_tasks: []
tags: [priority-medium, effort-medium, layer-frontend-pages]
milestone: 2
links:
  - 'https://www.figma.com/design/n1p6WCMVd4iinbuvOA2WjP/Designs?node-id=188-25012&m=dev'
  - 'https://www.figma.com/design/n1p6WCMVd4iinbuvOA2WjP/Designs?node-id=206-15608&m=dev'
  - 'https://www.figma.com/design/siumLgKOc9LLepEfbimyp3/Design-System---Stellar-Block-Explorer?node-id=360-1812&m=dev'
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-04-23
    status: backlog
    who: karolkow
    note: 'Updated per task 0154: tokens→assets rename throughout'
  - date: 2026-05-18
    status: active
    who: karolkow
    note: 'Promoted to active — bundled with 0073 on shared branch feat/0073-0074_frontend-account-and-asset-pages.'
  - date: 2026-05-18
    status: active
    who: karolkow
    note: 'Spec sync: paths corrected to web/ (no apps/); hooks to web/src/api/hooks/ per 0066; sub-component layout per 0069; type badge via Chip (no TypeBadge). File renamed tokens→assets.'
  - date: 2026-05-18
    status: active
    who: karolkow
    note: 'Added Figma links — assets list + assets detail frames + design-system file (Chip).'
  - date: 2026-05-18
    status: active
    who: karolkow
    note: 'Implemented assets list + detail. Type filter is chips without Native, and the asset-transactions table shows Ledger instead of Fee, matching Figma — acceptance criteria updated to match.'
  - date: 2026-05-18
    status: completed
    who: karolkow
    note: >
      Completed. Assets list + detail pages delivered on branch
      feat/0073-0074; verified 1:1 against Figma; typecheck/lint/build
      green. safeHttpUrl guard added for off-chain TOML URLs. Tests
      deferred to task 0226.
---

# Frontend: Assets list and detail pages

## Summary

Implement the Assets list page (`/assets`) and Asset detail page (`/assets/:id`). Covers native XLM, classic credit assets, SACs, and Soroban-native tokens in a unified browsing surface with clear type differentiation.

## Status: Completed

**Current state:** Implemented on branch
`feat/0073-0074_frontend-account-and-asset-pages`; archived pending merge.

## Context

The asset pages must unify all asset classes into one browseable surface while making their type differences explicit. Classic credit assets are identified by code + issuer, Soroban tokens by contract ID, native XLM is a singleton. Type badges (native / classic_credit / SAC / soroban) prevent user confusion.

### API Endpoints Consumed

| Endpoint                       | Query Params                                      | Purpose                                                              |
| ------------------------------ | ------------------------------------------------- | -------------------------------------------------------------------- |
| `GET /assets`                  | `limit`, `cursor`, `filter[type]`, `filter[code]` | Paginated asset list with type and code filters                      |
| `GET /assets/:id`              | none                                              | Asset detail: code, issuer/contract, type, supply, holders, metadata |
| `GET /assets/:id/transactions` | `limit`, `cursor`                                 | Paginated transactions involving this asset                          |

### Asset List Table Columns

| Column               | Display                                   | Notes                                                                                       |
| -------------------- | ----------------------------------------- | ------------------------------------------------------------------------------------------- |
| Asset Code           | Text                                      | Primary identifier text (e.g., "USDC", "XLM")                                               |
| Issuer / Contract ID | Truncated, linked                         | Classic: issuer linked to `/accounts/:id`. Soroban: contract ID linked to `/contracts/:id`. |
| Type                 | Badge (native/classic_credit/SAC/soroban) | `Chip` color-coded per type (task 0063). Prevents confusion between similar names.          |
| Total Supply         | Formatted number                          | Total supply of the asset                                                                   |
| Holder Count         | Integer                                   | Number of accounts holding this asset                                                       |

### Asset List Filters

| Filter     | Type                                    | Notes                     |
| ---------- | --------------------------------------- | ------------------------- |
| Type       | Chips: All types, Classic, SAC, Soroban | Filters by `filter[type]` |
| Asset Code | Text input / search                     | Filters by `filter[code]` |

- Filters additive, reflected in URL
- Filter change resets cursor
- Type filter rendered as chips per the Figma design; native excluded
  (the native asset is the single XLM entry)

### Asset Detail Fields

| Field                 | Display                                    | Notes                                                                    |
| --------------------- | ------------------------------------------ | ------------------------------------------------------------------------ |
| Asset Code            | Prominent header                           | Primary asset name                                                       |
| Issuer (classic)      | Full, copyable, linked to `/accounts/:id`  | IdentifierWithCopy (task 0062). Only for classic_credit/SAC.             |
| Contract ID (soroban) | Full, copyable, linked to `/contracts/:id` | IdentifierWithCopy (task 0062). Only for Soroban/SAC assets.             |
| Type Badge            | Prominent badge                            | `Chip` color-coded per type (task 0063). Must be obvious at top of page. |
| Total Supply          | Formatted number                           | Total asset supply                                                       |
| Holder Count          | Integer                                    | Number of holders                                                        |
| Deployed At Ledger    | Linked to `/ledgers/:sequence`             | Only for Soroban/SAC assets. IdentifierDisplay (task 0062).              |

### Asset Metadata (when available)

| Field              | Display | Notes                                        |
| ------------------ | ------- | -------------------------------------------- |
| Name               | Text    | Full asset name                              |
| Description        | Text    | Asset description                            |
| Icon               | Image   | Asset icon/logo. Placeholder if unavailable. |
| Domain / Home Page | Link    | External link to asset's website             |

- Metadata may be partially available. Tolerate missing fields gracefully.

### Asset Transactions Table Columns

Same as global transaction table conventions:
| Column | Display |
|---|---|
| Hash | Truncated, linked |
| Ledger | Linked to `/ledgers/:sequence` |
| Source Account | Truncated, linked |
| Operation Type | Label |
| Status | Badge |
| Timestamp | Relative |

- Fee column omitted per the Figma design

## Implementation Plan

> Structure follows task 0069 (transactions list): page entry files stay flat
> in `web/src/pages/` (router imports them), page-specific sub-components go in
> the route-named subdirectory `web/src/pages/assets/`, and data hooks go in
> `web/src/api/hooks/` per task 0066 convention.

### Step 1: Asset list query hook and page

Create the hook `web/src/api/hooks/useAssetsList.ts` and flesh out the existing
router stub `web/src/pages/AssetsListPage.tsx`:

- Fetches `GET /assets` with limit, cursor, type filter, code filter
- Filter controls: type chips, code text input
- Table with columns: asset code, issuer/contract ID, type badge, total supply, holder count
- Cursor-based pagination

### Step 2: Asset detail query hooks

Create `web/src/api/hooks/useAssetDetail.ts` and `web/src/api/hooks/useAssetTransactions.ts`:

- `useAssetDetail`: fetches `GET /assets/:id`, stale time 5 minutes
- `useAssetTransactions`: fetches `GET /assets/:id/transactions` with cursor

### Step 3: Asset detail summary

Create `web/src/pages/assets/AssetSummary.tsx`:

- Asset code as header
- Type badge (prominent, near top)
- Issuer (classic_credit/SAC) OR contract ID (Soroban/SAC) — full, copyable, linked
- Supply, holder count, deployed at ledger (Soroban/SAC only)

### Step 4: Asset metadata section

Create `web/src/pages/assets/AssetMetadata.tsx`:

- Name, description, icon, domain
- Graceful handling of missing fields (show what is available, hide empty sections)

### Step 5: Asset transactions section

Create `web/src/pages/assets/AssetTransactions.tsx`:

- Paginated transaction table, standard columns
- SectionHeader: "Transactions"

### Step 6: Page composition

Flesh out the existing router stub `web/src/pages/AssetDetailPage.tsx`:

- Composes: AssetSummary, AssetMetadata, AssetTransactions
- Each section in SectionErrorBoundary (task 0064)
- 404 state: "Asset not found"

## Acceptance Criteria

- [x] Asset list columns: asset code, issuer/contract ID, type badge (native/classic_credit/SAC/soroban), total supply, holder count
- [x] Filters: type chips (Classic/SAC/Soroban), code search. Reflected in URL.
- [x] Asset detail shows: code, issuer OR contract ID (copyable, linked), type badge (prominent), supply, holders, deployed at ledger (Soroban/SAC)
- [x] Type badge clearly distinguishes native, classic_credit, SAC, and Soroban assets
- [x] Metadata section tolerates partial availability (missing name/icon/description)
- [x] Asset transactions table with standard columns, cursor pagination
- [x] Classic credit asset issuer linked to `/accounts/:id`; Soroban contract linked to `/contracts/:id`
- [x] 404 state: "Asset not found"
- [x] Loading skeleton and error states per section

## Implementation Notes

Delivered on branch `feat/0073-0074_frontend-account-and-asset-pages`
(commits `6e3347f`, `1518020`, `dfef824`, `47d0bfb`).

- Hooks: `web/src/api/hooks/{useAssetsList,useAssetDetail,useAssetTransactions}.ts`.
- Pages: `web/src/pages/AssetsListPage.tsx`, `AssetDetailPage.tsx`
  (fleshed-out router stubs).
- Sub-components: `web/src/pages/assets/{AssetIcon,AssetFilters,AssetsTable,
AssetSummary,AssetMetadata,AssetTransactions}.tsx`, plus `assetType.ts`.
- Shared: `web/src/pages/url.ts` (`safeHttpUrl`) and the `detail/`
  primitives created with 0073.
- Verified 1:1 against the Figma frames by rendering against a mock API;
  typecheck, lint and build green. No automated tests — deferred to 0226.

## Issues Encountered

- **Off-chain URLs are untrusted**: `home_page` / `icon_url` come from
  SEP-1 TOML metadata; a `javascript:` URL would otherwise reach an `href`.
  Added `safeHttpUrl` to gate both an `href` and an image `src`.
- **No wrap mode in `IdentifierDisplay`**: a long issuer/contract id clips
  in the narrow asset-summary card. A proper `wrap` prop on the libs/ui
  component did not propagate cleanly through the nx project-reference /
  `customConditions` build (`nx typecheck` failed against a correct
  source/dist), so it was reverted; a documented CSS override in
  `AssetSummary` wraps the value instead.

## Design Decisions

### From Plan

1. **Unified asset surface** — native, classic credit, SAC and Soroban
   assets in one list/detail flow; per-section `SectionErrorBoundary`,
   cursor pagination, libs/ui primitive reuse.
2. **Metadata tolerates partial availability** — only present TOML fields
   render; an empty metadata section shows a short placeholder.

### Emerged

3. **Type filter is chips, not a dropdown** — the Figma filter is a chip row
   (All types / Classic / SAC / Soroban) and excludes Native; the spec said
   a dropdown including native. Figma took priority; acceptance criteria
   updated.
4. **Asset-transactions table shows Ledger, omits Fee** — matches the Figma
   asset-detail table; the original spec listed Fee and no Ledger.
5. **Type badge via generic `Chip`** — colour-coded (blue native, neutral
   classic, violet SAC, emerald Soroban); task 0063 shipped `Chip`, not a
   named `TypeBadge`.
6. **`safeHttpUrl` URL guard** — off-chain TOML `home_page` / `icon_url` are
   validated to `http(s)` before reaching an `href` / `src`.

## Notes

- Asset identity is the most confusing area for users: classic_credit assets share codes across issuers, Soroban assets are identified by contract. Type badges and display formatting are critical.
- The asset detail page serves as a discovery path into the broader explorer via transaction links.
- Figma frames may still carry legacy "tokens" naming; code, routes, and types use "assets" per task 0154 — do not reintroduce "tokens".
