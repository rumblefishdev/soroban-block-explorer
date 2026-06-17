---
id: '0296'
title: 'BUG: NFT/event extraction completeness — packed-data NFT event shapes silently dropped + CAP-67 address robustness'
type: BUG
status: active
related_adr: []
related_tasks: ['0283', '0231', '0259']
tags:
  [
    xdr-parser,
    nft,
    extraction-completeness,
    layer-data,
    priority-medium,
    effort-small,
  ]
links: []
history:
  - date: 2026-06-16
    status: backlog
    who: karolkow
    note: >
      Spawned from 0283 future work. Carries the NFT event-shape fix + CAP-67
      verification done during the 0283 session-3 deep-dive but deliberately
      kept OUT of 0283 scope. The code is parked in a git STASH on the fix/0283
      worktree — find by message `git stash list | grep '0296: NFT event-shape'`
      (base commit 1d421ba5). The stash also carries unrelated snapshot files;
      extract ONLY crates/xdr-parser/src/{nft.rs,scval.rs}.
  - date: 2026-06-16
    status: active
    who: karolkow
    note: >
      Activated for implementation (promote-task). Branch fix/0296. Step 1:
      recover the parked stash from the fix/0283 worktree
      (`git stash list | grep '0296: NFT event-shape'`, base 1d421ba5),
      extract ONLY crates/xdr-parser/src/{nft.rs,scval.rs}.
---

# BUG: NFT/event extraction completeness

## Summary

The NFT event parser silently dropped real NFTs whose events use a non-standard
shape, and CAP-67 (Protocol 23) address variants needed a robustness check. Both
were done during the 0283 session-3 deep-dive and validated against real
on-chain data; the work is parked here to land on its own branch, outside the
0283 reclassification scope.

## Context

Spawned from **0283**. Two pieces:

1. **NFT event-SHAPE gap (the substantive fix).** `detect_nft_events`
   (`crates/xdr-parser/src/nft.rs`) handled only **Shape A** (SEP-50 NFT /
   SEP-41: `topics=[sym, from, to]`, `data=token_id`) and SILENTLY dropped
   **Shape B** (packed: `topics=[Symbol]`, `data=Vec(addr…, token_id)` — e.g.
   Bachini / ERC-721 ports). Dropped NFTs never reach `nfts_pending` → a silent,
   uncountable data-loss class (NOT the 0283 classification gap; the example
   contract HAS a wasm). Fix adds a unified `extract_args` (Shape A + B) + a
   `tracing::warn!` **tripwire** on symbol-matched-but-unparsed. A speculative
   "Shape A2" (token_id-as-extra-topic) was considered and CUT (no SEP defines
   it, no on-chain instance found). **Map-data shape is still DEFERRED** — the
   tripwire surfaces real cases.

   - **Empirical sweep (getEvents, ~1-day window, 255 transfer/mint/burn
     events):** 98% Shape A (handled), **0 Shape B / 0 A2** (both historical —
     Bachini is a 2024 contract, out of the RPC retention window), and the only
     dropped shape (map, 5 events) came from a **non-NFT** contract (no
     `owner_of`/`token_uri`). **Implication for prioritization:** Shape B/A2
     recover mainly HISTORICAL / long-tail NFTs (needs re-parse, Step 3), and
     map-shape is rare-and-maybe-non-NFT — so the map deferral is justified;
     don't over-invest until the tripwire surfaces a real NFT map emitter.
   - **Validated** against the real on-chain Bachini Mint
     (`CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY`):
     `topics=[Symbol("Mint")]`, `data=Vec[Address, I128]`; raw XDR → parser →
     row. Wasm independently confirmed NFT (`owner_of`+`token_uri`).
   - **Authoritative grounding:** Shape A = [SEP-50](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0050.md)
     - [SEP-41](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md);
       Shape B = non-standard but chain-verified.

2. **CAP-67 address robustness (verified clean).** stellar-xdr 26 decodes all
   ScAddress variants (MuxedAccount/ClaimableBalance/LiquidityPool); a test
   confirms `scval_to_typed_json` renders them to non-empty StrKeys (a stale JS
   SDK crashed on these — our Rust parser does not). The test rides in the
   parked stash (`scval.rs`).

### Magnitude (evidence)

- **Prod (2026-06-16, via `chq`): of 125 NFT-classified collections, only 40
  have any `nfts_pending` rows (11,214 tokens) — 85 of 125 (68%) have ZERO
  surfaced tokens.** That gap is the strongest signal the event-shape drop is
  broad, not a Bachini one-off (caveat: some of the 85 may be genuinely
  inactive/empty, not all shape-victims — confirm by re-parsing a sample).
- **Recent-window sweep (getEvents, 255 events):** 98% Shape A (the handled
  standard), 0 Shape B / 0 A2 (historical-only), and the lone dropped shape
  (map-data, 5 events) came from a NON-NFT contract. So Shape-B fixes mostly
  recover HISTORICAL / lower-velocity NFTs (re-parse), while map-data is rare
  and may be non-NFT — let the tripwire confirm before investing in map.

## Implementation Plan

### Step 1 — recover the parked work (git stash)

On the fix/0283 worktree: `git stash list | grep '0296: NFT event-shape'` →
`git checkout <ref> -- crates/xdr-parser/src/nft.rs crates/xdr-parser/src/scval.rs`
(base 1d421ba5; extract ONLY those two paths — the stash carries unrelated
snapshot files). Confirm `cargo test -p xdr-parser` green + clippy clean (was
244 green at parking time). **If the stash is lost**, the work is re-derivable
from the Shape-B spec + the real Bachini event documented below.

### Step 2 — map-data shape (tail)

Implement the map-shaped event-data layout once the tripwire surfaces real prod
samples (one strict live NFT in the chain sample emitted map-data). Don't guess
key names blind.

### Step 3 — backfill already-dropped rows

The fix is parse-time; NFTs dropped historically need a **raw-S3 re-parse** to
materialize their `nfts_pending` rows.

### Step 4 — optional gold-standard

Run a real muxed/CAP-67 event end-to-end through the full parser (the unit test
constructs ScVal directly, bypassing wire decode).

## Acceptance Criteria

- [ ] Patches applied, `xdr-parser` tests green + clippy clean
- [ ] Map-data shape handled, or tripwire-confirmed-absent in prod
- [ ] Re-parse backfill plan for historically-dropped NFTs
- [ ] (optional) real CAP-67 event end-to-end test
