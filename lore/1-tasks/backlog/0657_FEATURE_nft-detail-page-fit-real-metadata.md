---
id: '0657'
title: 'NFT detail page: show what the metadata holds, hide what it does not'
type: FEATURE
status: backlog
related_adr: ['0043']
related_tasks: ['0076', '0229', '0282', '0486', '0654']
tags: [frontend, nfts, ux, priority-medium, effort-medium]
links: []
history:
  - date: '2026-10-26'
    status: backlog
    who: karolkow
    note: >
      Created from a UX audit of the NFT detail page against live mainnet
      data (2026-10-23). The page was built for OpenSea-style picture NFTs
      with traits; 97.5% of the sampled NFTs have no traits, and the field
      that says what most of them are (`description`) is never rendered.
---

# NFT detail page: show what the metadata holds, hide what it does not

## Summary

The NFT detail page (`web/src/pages/NftDetailPage.tsx`, `nft-detail/*`) follows
the Figma design from task 0076, which assumes every NFT is a picture with
OpenSea-style `attributes`. On Stellar mainnet most NFTs are positions
(SushiSwap V3, XOXNO lending), domains or registry entries. The page shows
empty boxes for what they lack and drops what they have. Rebuild it around
the metadata shapes the data actually holds, on the pattern the asset page
already uses: only sections and rows with data are shown.

## Stan teraz

- Done: audit with measurements (below); no code.
- Next: choose the shape (see Not yet decided), then a clickable prototype on
  the three real NFTs listed under Test NFTs, then the implementation.
- In force: description and every metadata value come from the token's issuer
  and render as plain text — no HTML, no auto-linked URLs.

## Context

Measured 2026-10-23 through the production API (`/v1/nfts` newest 600, then
`/v1/nfts/{contract}/{token}` for one NFT per collection; 11 collections):

| Collection                    | NFTs in sample | `attributes`  | `description`  | Media                        |
| ----------------------------- | -------------- | ------------- | -------------- | ---------------------------- |
| SushiSwap V3 Positions NFT-V1 | 469            | none          | yes, not shown | none                         |
| XOXNO Lending Position        | 90             | metadata null | —              | url                          |
| XLM Domains                   | 13             | 2             | yes, not shown | PNG 503, `animation_url` 200 |
| Stroopies                     | 9              | none          | —              | `data:` SVG                  |
| Stellar PFP                   | 2              | 2             | yes, not shown | url                          |
| Agent Registry                | 1              | none          | yes, not shown | url                          |
| 5 others                      | 16             | metadata null | —              | mostly none                  |

15 of 600 (2.5%) carry traits. A sample of the newest NFTs only — older
collections and rare metadata shapes are not covered.

### Findings

1. **Empty sections take most of the page.** The Traits card renders on every
   NFT ("No traits for this NFT" / "Metadata unavailable", ~170 px), and a
   missing image leaves a 308 px placeholder that says "No media available"
   and "There is no image". On a SushiSwap position the only content is four
   Details rows and one transfer row.
2. **`description` is never rendered.** For SushiSwap it is the answer to
   "what is this": pool pair, fee tier, pool and token addresses, and the
   issuer's warning that token symbols may be imitated.
3. **Trait tiles break on real values.** XLM Domains traits are a 64-char hash
   and a G-address, set in large bold centred text in narrow tiles — six
   wrapped lines each. The `Owner` trait repeats the Current owner row and is
   not a link.
4. **An issuer outage reads as our error.** XLM Domains `image` (PNG) returns
   503 from the issuer's host; the page shows a red "Failed to load media".
   The same metadata's `animation_url` (SVG) returns 200 and is never tried.
5. **Other metadata fields are invisible** — `animation_url`, Agent Registry's
   `services`, `x402`, `type`. No standard defines them, so they are shown
   raw, not interpreted.
6. **Own copies of shared parts.** The page has its own `Breadcrumb` and `Row`
   next to `detail/PageBreadcrumb` and `detail/SummaryRow`, and `Card` +
   `TableSectionHeader` where siblings use `detail/SectionCard`; it looks
   slightly different from the asset and account pages.
7. **The collection name is plain text** — no way from an NFT to the rest of
   its collection (overlaps 0486, which owns the destination view).

### Test NFTs (mainnet)

- SushiSwap position, no media, description only:
  `CARTUL5AWDZYBSN7HUUJZSKCAKCIAKM7M54Z76G6KRYCK4XPR3OHUQZ4/1290`
- XLM Domains, traits + failing PNG + working `animation_url`:
  `CCHHGIOBKTFUS4XFJ3VGPOHT3ZWT3NYMFBPA6RFXFHM62Y6R4SG7CK2U/134`
- Agent Registry, non-standard fields:
  `CBGPDCJIHQ32G42BE7F2CIT3YW6XRN5ED6GQJHCRZSNAYH6TGMCL6X35/68`
- Stroopies, inline `data:` SVG image:
  `CDC5ZX53PWDPVS5AGZPHJ6WHFPB66AZFFEHK4NCFH2AR7ZZQUKOGJFGB/8`

## Not yet decided

**Shape of the page** — the owner picks after a prototype:

- From scratch: only sections with data; description under the title
  (clamped, "Show more"); one Metadata card of label → value rows
  (`SummaryRow`; addresses and hashes shortened and linked like Current
  owner) with a collapsible raw-JSON row (`detail/DisclosureRow`); media
  falls back to `animation_url`, then to a neutral placeholder that names the
  issuer's host as the failing side; no media box when there was never media.
  ~1 day plus prototype.
- Naive: today's layout minus the empty Traits card, traits as rows,
  description under the title. ~2–3 h; findings 4, 5 and 7 stay.

Also open: whether the `animation_url` fallback belongs in the frontend or in
the API's `media_url` resolver (0282 owns resolver quality).

## Out of scope

- Trait rarity (0229) — with 2.5% of NFTs carrying traits, re-weigh that task
  before building it.
- The collection view itself (0486); this task only links to it.

## Acceptance Criteria

- [ ] An NFT with no traits and no media (SushiSwap #1290) shows no empty
      Traits card and no media placeholder box.
- [ ] `description` renders as plain text with line breaks kept.
- [ ] XLM Domains #134 shows its traits as rows that do not wrap a hash into
      a column; an address value is a shortened link.
- [ ] XLM Domains #134 shows an image (from `animation_url`) while the PNG
      host returns 503; a media failure is not styled as an error of ours.
- [ ] Every metadata field is reachable on the page (raw view at least).
- [ ] Breadcrumb and summary rows use the shared `detail/*` components.
- [ ] Checked at 375 px and desktop width on the four test NFTs.
- [ ] **Docs updated** — `docs/architecture/**` frontend data contract:
      updated if the API response changes, else `N/A — frontend only`.
- [ ] **API types regenerated** — only if `crates/api/**` changes.
