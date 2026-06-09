---
id: '0280'
title: 'NFT media-URL quality: content-type validation + token_image() fallback'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0231']
tags: ['phase-future', 'effort-small', 'priority-low']
links: []
history:
  - date: 2026-06-10
    status: backlog
    who: claude
    note: 'Spawned from 0231 NFT enrichment. url-fallback for media_url landed (option A); content-type validation (B) + the token_image() fetch fallback are deferred here.'
---

# NFT media-URL quality: content-type validation + token_image() fallback

## Summary

Harden the NFT `media_url` resolver against two known gaps that 0231 left at
the cheap-but-imperfect tier. Both are blocked on **a larger real-NFT sample**
than the single verified mainnet contract available today (`CDA5FGE4…`), so
they are parked until a prod NFT population exists to validate against.

## Context

Spawned from **0231** (CH SEP-1 + NFT enrichment). The NFT resolver
(`crates/enrichment-shared/src/enrich_and_persist/nft_token_uri.rs::extract_columns`)
reads `media_url` from the `token_uri()` metadata JSON's `image` field, with a
fallback to the `url` field (added in 0231 — the `CDA5FGE4` prototype carries
the image CID under `url`, not `image`). Two quality gaps remain:

1. **No content-type validation.** Neither `image` nor the `url` fallback is
   verified to actually be an image — only the `is_safe_https_url` scheme guard
   (https-only) runs. A non-image URL (esp. via `url`, which in the OpenSea
   convention is often an `external_url` website, not the image) lands in
   `media_url` and renders as a broken `<img>`. n=1 today, so unmeasurable.
2. **`token_image()` not used.** `CDA5FGE4` also exposes a separate 0-arg
   `token_image()` contract entrypoint returning the image URL directly. 0231
   chose NOT to call it (+1 RPC/token, and the `url` field already covered the
   one known case). If image-behind-`token_image()` (with no usable JSON field)
   turns out common in a real population, a fetch fallback is warranted.

## Implementation

- **Content-type validation (option B):** when a `media_url` candidate is
  resolved (from `image` or `url`), issue a lightweight `HEAD` (fallback `GET`
  with range) and accept only `Content-Type: image/*`; otherwise write the `''`
  sentinel. Applies to BOTH `image` and the `url` fallback. Mind the new
  failure mode: a transiently-down image host would sentinel a valid URL —
  classify host-unreachable as transient (retry), not permanent, OR keep the
  URL and only reject on a definitive non-image content-type.
- **`token_image()` fallback:** when neither `image` nor `url` yields a usable
  media URL, simulate `token_image()` (0-arg, then 1-arg) — a near-verbatim
  copy of `simulate_token_uri_with_fallback` in
  `crates/enrichment-shared/src/nft_token_uri/client.rs` — and resolve/guard
  the returned URI the same way. Gate behind "image + url both absent" so the
  extra RPC only fires for the minority that need it.
- **Prerequisite:** a real mainnet NFT sample (>1 contract). Blocked on NFT
  discovery (see 0231 notes — Soroban NFTs are structurally scarce; the
  Meridian 2025 SEP-50 collection is the best lead but its contract id is
  unpublished). Re-evaluate commonality before building either.

## Acceptance Criteria

- [ ] `media_url` candidates validated as `image/*` (content-type), with a
      transient-vs-permanent classification for the validation fetch itself
- [ ] `token_image()` fallback resolves media when the JSON carries no usable
      `image`/`url`, behind the both-absent gate
- [ ] Decision recorded on whether the `url` fallback should be kept,
      tightened, or gated once a real sample exists
- [ ] Unit tests for: non-image content-type → sentinel; `token_image()`
      path; the both-absent gate
