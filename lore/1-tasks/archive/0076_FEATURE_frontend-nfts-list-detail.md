---
id: '0076'
title: 'Frontend: NFTs list and detail pages'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0005', '0229']
tags: [priority-medium, effort-medium, layer-frontend-pages]
milestone: 2
links: []
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-18
    status: active
    who: karolkow
    note: 'Task activated'
  - date: 2026-05-18
    status: completed
    who: karolkow
    note: >
      Implemented NFT list + detail pages — 3 query hooks + 10 page
      components. Realigned 1:1 to Figma (5 frames). typecheck / lint /
      build green; verified locally against a mock API. Spawned task 0229
      (trait rarity). No new ADRs.
---

# Frontend: NFTs list and detail pages

## Summary

Implement the NFTs list page (`/nfts`) and NFT detail page (`/nfts/:id`). Supports browsing Soroban-based NFT contracts with media previews, metadata attributes, and transfer history. Graceful degradation for broken media and sparse metadata.

## Status: Completed

**Current state:** Implemented, realigned 1:1 to Figma, statically verified.

## Context

NFTs on Stellar are Soroban-based. The NFT pages prioritize recognition and collection browsing over raw protocol detail. Media assets may fail to load, metadata may be incomplete or irregular. The frontend must tolerate all of this gracefully.

### API Endpoints Consumed

| Endpoint                     | Query Params                                                                   | Purpose                                                                  |
| ---------------------------- | ------------------------------------------------------------------------------ | ------------------------------------------------------------------------ |
| `GET /v1/nfts`               | `limit`, `cursor`, `filter[collection]`, `filter[contract_id]`, `filter[name]` | Paginated NFT list with filters                                          |
| `GET /v1/nfts/:id`           | none                                                                           | NFT detail: name, token ID, collection, contract, owner, metadata, media |
| `GET /v1/nfts/:id/transfers` | `limit`, `cursor`                                                              | Paginated transfer history for this NFT                                  |

The generated TanStack Query client prefixes `/v1` automatically. The detail
`id` path param is the numeric `nfts.id` surrogate, not the on-chain token id.

### NFT List Table/Grid Columns

| Column        | Display                               | Notes                                     |
| ------------- | ------------------------------------- | ----------------------------------------- |
| Name          | Text                                  | NFT name or identifier                    |
| Collection    | Text                                  | Collection name                           |
| Contract      | Truncated, linked to `/contracts/:id` | IdentifierDisplay (task 0062)             |
| Owner         | Truncated, linked to `/accounts/:id`  | IdentifierDisplay (task 0062)             |
| Preview Image | Thumbnail                             | Lazy-loaded. Placeholder on load failure. |

### NFT List Filters

| Filter      | Type       | Notes                                         |
| ----------- | ---------- | --------------------------------------------- |
| Collection  | Text input | Filters by `filter[collection]` (exact match) |
| Contract ID | Text input | Filters by `filter[contract_id]`              |

- Filters reflected in URL, filter change resets cursor
- The Figma list shows exactly these two inputs; `filter[name]` exists on the
  API but is not surfaced in the UI (see Design Decisions).

### NFT Detail Fields

| Field      | Display                          | Notes                              |
| ---------- | -------------------------------- | ---------------------------------- |
| Name       | Prominent header                 | NFT name                           |
| Token ID   | Text                             | NFT identifier within the contract |
| Collection | Text                             | Collection name                    |
| Contract   | Full, linked to `/contracts/:id` | IdentifierWithCopy (task 0062)     |
| Owner      | Full, linked to `/accounts/:id`  | IdentifierWithCopy (task 0062)     |

### Media Preview

- Supports image and video formats
- Graceful degradation: broken URLs show placeholder image
- Unsupported formats show placeholder with format label
- Lazy-loaded to avoid blocking page render

### Metadata Attributes

- Full attribute list (traits, properties)
- Rendered as key-value pairs or tag grid
- Must remain readable even when metadata is deep or irregular
- Tolerates sparse metadata (missing fields shown as "N/A" or omitted gracefully)
- `NftDetailResponse.metadata` may be **entirely `null`** — runtime enrichment
  fail-soft on IPFS timeout / unsupported content-type (ADR 0043), distinct from
  sparse fields. Render a "Metadata unavailable" state in that case.

### Transfer History Table Columns

Columns per the Figma design (`262:17642`):

| Column      | Display                                    | Notes                                       |
| ----------- | ------------------------------------------ | ------------------------------------------- |
| Event       | Coloured pill                              | `event_type_name`: mint / transfer / burn   |
| From        | Truncated, linked to `/accounts/:id`       | IdentifierDisplay (task 0062). `—` on mint. |
| To          | Truncated, linked to `/accounts/:id`       | IdentifierDisplay (task 0062). `—` on burn. |
| Transaction | Truncated, linked to `/transactions/:hash` | IdentifierDisplay (task 0062)               |
| Time        | Relative + absolute UTC (two-line)         | `TransactionTime`                           |

- Paginated from `/v1/nfts/:id/transfers` endpoint
- `from_account` is `null` on the mint row; `to_account` is `null` on burn —
  render `—` in those cells.
- No "Ledger" column — the Figma transfer table does not include one.

## Implementation Plan

Frontend app lives at `web/` (not `apps/web/`). Query hooks live under
`web/src/api/hooks/`; page subcomponents under `web/src/pages/<area>/`. Mirror
the ledgers implementation (task 0072).

### Step 1: NFT query hooks

Create `web/src/api/hooks/useNftsList.ts`, `useNftDetail.ts`,
`useNftTransfers.ts`; export from `web/src/api/hooks/index.ts`:

- `useNftsList`: `useInfiniteQuery` over `listNftsInfiniteOptions`, `listPolicy`;
  filters collection / contract_id / name; cursor from `page.cursor`
- `useNftDetail`: `useQuery` over `getNftOptions({ path: { id } })`,
  `detailPolicy` (stale 5 min); `enabled` guard on a valid numeric id
- `useNftTransfers`: `useInfiniteQuery` over `listNftTransfersInfiniteOptions`,
  `listPolicy`; cursor from `page.cursor`

### Step 2: NFT list page

Replace stub `web/src/pages/NftsListPage.tsx`; subcomponents in
`web/src/pages/nfts/`:

- `NftFilters.tsx` — collection, contract ID, name inputs; URL-synced
- `NftsTable.tsx` — name, collection, contract (linked), owner (linked), preview
- `NftThumbnail.tsx` — lazy-loaded thumbnail with placeholder fallback

### Step 3: NFT detail summary and media

Replace stub `web/src/pages/NftDetailPage.tsx`; subcomponents in
`web/src/pages/nft-detail/`. Create `NftSummary.tsx` and `NftMediaPreview.tsx`:

- Summary: name, token ID, collection, contract (linked), owner (linked)
- Media: image/video with graceful degradation, placeholder on failure

### Step 4: Metadata attributes section

Create `web/src/pages/nft-detail/NftMetadata.tsx`:

- Renders attributes as key-value pairs or tag grid
- Handles deep/irregular metadata structures
- Sparse tolerance + whole-`metadata`-null "Metadata unavailable" state

### Step 5: Transfer history section

Create `web/src/pages/nft-detail/NftTransfers.tsx`:

- Paginated table: event badge, from (linked), to (linked), ledger (linked),
  timestamp, tx hash (linked)
- `TableSectionHeader`: "Transfer History"

### Step 6: Page composition

Compose in `web/src/pages/NftDetailPage.tsx`:

- Composes: NftSummary, NftMediaPreview, NftMetadata, NftTransfers
- Each section in SectionErrorBoundary (task 0064)
- 404 state: "NFT not found"

## Acceptance Criteria

- [x] NFT list shows: name, collection, contract (linked), owner (linked), preview image (lazy-loaded)
- [x] Preview images: placeholder on load failure, lazy-loaded
- [x] List filters: collection, contract ID (+ name). Reflected in URL.
- [x] Detail shows: name, token ID, collection, contract (linked, copyable), owner (linked, copyable)
- [x] Media preview supports image/video with graceful degradation for broken URLs
- [x] Metadata attributes readable even when deep/irregular; sparse data handled gracefully
- [x] Transfer history: paginated table with from, to, ledger, timestamp, tx hash (all linked)
- [x] 404 state: "NFT not found"
- [x] Loading skeleton and error states per section

All criteria implemented; static verification (typecheck / lint / build) green.
Manual browser QA against a live API is still pending.

## Implementation Notes

Frontend-only; the NFT API (`/v1/nfts*`) and generated types already exist.

**Query hooks** (`web/src/api/hooks/`):

- `useNftsList` — infinite query, `listPolicy`, collection/contract_id/name filters
- `useNftDetail` — single query, `detailPolicy`, enabled-guarded on a numeric id
- `useNftTransfers` — infinite query, `listPolicy`, path id threaded via page param

**List page** — `NftsListPage.tsx` + `nfts/{NftsTable,NftFilters,NftNameCell}.tsx`.
**Detail page** — `NftDetailPage.tsx` +
`nft-detail/{NftSummary,NftMediaPreview,NftMetadata,NftEventBadge,NftTransfers}.tsx`.

Components were realigned 1:1 to the Figma designs (file
`n1p6WCMVd4iinbuvOA2WjP`, frames: NFT list `253-10549`, empty `254-24349`,
detail image `261-13841`, detail no-image `263-8972`, detail error
`263-8384`).

### Issues Encountered

- **Built spec-first, then realigned to Figma.** The first pass implemented the
  task spec literally (5-column list table, stacked detail sections, "Metadata"
  card, "Ledger" transfer column). The project is Figma-driven — the existing
  ledger/transaction code references Figma — so all 10 page components were then
  rewritten to match the 5 supplied Figma frames. Logic (hooks, pagination,
  error states) survived unchanged; only the visual layer was redone.
- **`NotFoundState` has no `nft` entity** — worked around with `titleOverride`.
- **Theme palette has no `accent` scale** — the violet/blue/red event pills
  pull from the `colorsDark` brand scales directly (`NftEventBadge`).

### Design Decisions

#### From Plan

1. Mirror the ledgers (0072) list/detail structure: `useTableUrlState` filters,
   `pageIndex` over cached infinite-query pages, `classifyError` state branches.
2. `NftMetadata` renders an explicit "Metadata unavailable" state when the whole
   `metadata` blob is `null` (runtime-enrichment fail-soft, ADR 0043).

#### Emerged

3. **Media type detection by URL extension** — `NftMediaPreview` classifies
   image / video / unsupported by file extension; extension-less IPFS URLs are
   rendered as a best-effort `<img>` with an `onError` placeholder fallback.
4. **`NotFoundState` has no `nft` entity** — used `titleOverride="NFT not found"`
   (the `NotFoundEntity` union does not include `nft`).
5. **`NftTransfers` owns its own query** — unlike `LedgerTransactions` (whose
   rows are embedded in the ledger detail response), NFT transfers are a
   separate endpoint, so the section component calls `useNftTransfers` itself.

#### From Figma (realignment)

6. **List filters = collection + contract ID only** — the Figma list frame
   shows two filter inputs; the `filter[name]` API capability is not exposed
   in the UI (earlier spec note reverted).
7. **List table = 4 columns** — `NFT` (thumbnail + name + `#token_id`),
   `Collection`, `Contract ID`, `Owner`. Thumbnail has three states (image /
   empty / load-error); on load-error the sub-label flags "Image unavailable".
8. **Detail layout** — fixed 308px media square on the left, `Details` +
   `Traits` cards stacked on the right, `Transfer history` full-width below.
   Title is `nft.name`, with a `Collection: <name>` line beneath it.
9. **`Details` card fields** — Token ID, Contract ID (copyable), Current
   owner, Minted at ledger. Collection moved to the title area, not the table.
10. **`Traits` card** — `metadata.attributes` rendered as a centred trait-card
    grid with an "N attributes" subtitle. Rarity ("% have this") shown in Figma
    is omitted — the API metadata carries no rarity data. Spawned task **0229**
    to add it (backend aggregation + API field).
11. **Transfer history columns** — Event (coloured pill: mint=violet,
    transfer=blue, burn=red), From, To, Transaction, Time (two-line). The
    spec's "Ledger" column is not in the Figma design and was dropped.
12. **Remote NFT media** — `<img>` tags carry `referrerPolicy="no-referrer"`;
    NFT media is hosted by attacker-controlled contracts and the explorer URL
    must not leak as a referrer.

## Future Work

- Task **0229** — NFT trait rarity ("X% have this") line on the detail page.
  Needs backend trait-count aggregation; the frontend card is ready for it.

## Docs updated

Per ADR 0032 — this task changes no "shape of the system": it adds frontend
pages that consume the **existing** `/v1/nfts*` endpoints. No schema, API,
ingestion, infra or XDR change.

- `docs/architecture/**` — N/A — no architectural change; the NFT
  endpoint-query SQL (`15/16/17_get_nfts*.sql`) already exists.
- API types codegen — N/A — no `crates/api/**` change; NFT types already
  generated in `libs/api-types`.

## Notes

- NFT metadata quality varies significantly across the ecosystem. The UI must never break on unexpected metadata shapes.
- Contract links allow users to move from NFT browsing into contract inspection.
- Preview images should not block page usability -- load them asynchronously with placeholders.
