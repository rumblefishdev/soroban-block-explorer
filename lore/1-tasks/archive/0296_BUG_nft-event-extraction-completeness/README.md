---
id: '0296'
title: 'BUG: NFT/event extraction completeness — packed-data NFT event shapes silently dropped + CAP-67 address robustness'
type: BUG
status: completed
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
  - date: 2026-06-17
    status: completed
    who: karolkow
    note: >
      Parser completeness fix done. Recovered the parked stash (Shape A/B + tripwire +
      CAP-67 test) and extended it with Shape C `map{token_id}` (canonical OZ/SEP-50) +
      `consecutive_mint` range expansion + a fungible-map disambiguation guard. 265
      xdr-parser lib tests green (incl. 3 real-mainnet-XDR regression tests), clippy +
      fmt clean; downstream indexer/db-clickhouse/backfill-runner green. Verified
      3-layer (prod CH + code-blind SEP/OZ docs + live RPC) and reviewed by 5 agents
      (code/simplify×2/devil/checklist) — ship, no blocker. User-facing recovery needs
      follow-ups (deploy + 0283 reclassify + raw-S3 backfill) — see Future Work.
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
   tripwire surfaces real cases. **[SUPERSEDED 2026-06-17: prod shows `map{token_id}`
   is the canonical OZ shape and the dominant real-NFT drop; it IS now implemented —
   see §Verification update below.]**

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
  **[SUPERSEDED 2026-06-17 — this 1-day-window read was wrong: all-time prod shows
  `map{token_id}` is the DOMINANT real-NFT shape (16 contracts) and it is now
  handled. See §Verification update below.]**

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

- [x] Patches applied — stash recovered (Shape A/B + tripwire + CAP-67 test) and
      extended with `map{token_id}` + `consecutive_mint` + fungible-guard. **265
      `xdr-parser` lib tests green** (incl. 3 real-mainnet-XDR regression tests),
      **clippy clean**, **`cargo fmt` clean**.
- [x] Map-data shape handled — `map{token_id}` (canonical OZ reference-impl shape /
      SEP-50 NFT: addresses in topics, token_id in the data map) for mint/transfer/burn,
      plus `consecutive_mint` range expansion, plus a `map{amount,to_muxed_id}` fungible
      disambiguation guard; tripwire kept for genuinely-unknown shapes. Per-contract
      RPC-verified: **23 of 24** newly-recovered contracts are NFTs (`owner_of`/`token_uri`
      in wasm), all dropped by the old parser; the 24th (`CBCTZAZJ…`, a mint/withdraw
      contract) is a token_id-heuristic false-positive — contained in `nfts_pending` by
      the WASM classifier, never reaching hot. Proof: real-XDR Rust tests in `nft.rs`.
- [x] Re-parse backfill — **plan documented** (note §Recommendation item 6; below).
      Execution (raw-S3 re-parse to materialize historically-dropped rows) is an ops
      follow-up, **not done here**.
- [ ] (optional) real CAP-67 event end-to-end — `scval.rs` tests ScAddress rendering
      (all CAP-67 variants → StrKeys); full wire-decode e2e deferred (optional).

## Implementation Notes (2026-06-17)

Files: `crates/xdr-parser/src/nft.rs` (Shape A/B from stash + new Shape C
`map{token_id}`, `try_parse_consecutive_mint` with range guard `MAX_CONSECUTIVE_RANGE`,
`is_fungible_map` disambiguation, `maybe_tripwire`), `crates/xdr-parser/src/scval.rs`
(CAP-67 rendering test, from stash). Tests added: map-shape + consecutive*mint edge
cases + 3 real-mainnet-XDR regression tests (`detect_real_mainnet*\*`). Parser change
only — downstream persist/0283 routing unchanged; verified green against indexer +
db-clickhouse + backfill-runner. **Not committed.** (Throwaway RPC verifier scripts
were kept OUT of the repo — the chq/RPC verification recipe lives in the S-note.)

## Verification update (2026-06-17) — scope correction

Verified at three independent layers (prod ClickHouse, code-blind standards +
OpenZeppelin source, **live Stellar RPC** decoding raw XDR). Full evidence +
numbers + reproducible queries: [notes/S-prod-verification-map-data-dominant.md](notes/S-prod-verification-map-data-dominant.md).
Implementation proof = the real-mainnet-XDR Rust tests (`detect_real_mainnet_*` in
`crates/xdr-parser/src/nft.rs`); the RPC/chq verification recipe lives in the S-note.

**The parser drops 34 of 72 NFT-minting contracts chain-wide (exact):** 20 use
`data = vec[…]` (ERC-721 ports) and 14 use `data = map{"token_id"}` — the canonical
OpenZeppelin / SEP-50 shape (the `#[contractevent]` macro defaults to map-by-field-name),
confirmed live on mainnet (`CARTUL5A` mint `token_id 133`, `CCHHGIOB` mint `token_id 93`).
The "map = 5 events, non-NFT" reading in the Context section is a 1-day-RPC-window
artifact (retention is 7 days; the NFT tail is historical). Corrected plan:

- **Apply the parked stash** → recovers the 20 `vec` ports (Shape B) + tripwire —
  the LARGER dropped group, not marginal.
- **Raise priority: also handle `map{"token_id"}`** (14 contracts, currently deferred)
  — the canonical OZ/SEP-50 shape; trivial, key is literally `token_id`. Stash + map
  together recover all 34 dropped.
- **Mandatory disambiguation guard:** NFT iff data carries `token_id`; treat
  `amount`/`to_muxed_id` / a 4th `sep0011_asset` topic as fungible. Chain has 147.9M
  fungible `map{amount,to_muxed_id}` mints vs 890 NFT `map{token_id}` — without the
  guard a map handler mis-ingests ~5,580 fungible contracts.
- **New gap: `consecutive_mint`** (OZ Consecutive / EIP-2309) — `data=[from_id,to_id]`
  range, 8 contracts on prod, expand to N tokens. Not covered by stash or plan.
- Exact split: 72 minters = 36 scalar-only (reach pending) + 14 map-only + 20 vec-only
  - 2 mixed; 38 reach pending, 34 dropped (20 vec + 14 map), 1 classified, 0 hot.
- token_id width: accept u32→u256, store as string.
- CAP-67 scval address rendering is already correct (`stellar-xdr 26`) → test-only.

**Dependency / what 0296 ALONE delivers (sharpened per devil's-advocate review):**
the parser fix moves these events from _silently dropped_ → `nfts_pending` quarantine.
It does **not** make them user-visible. The hot `nfts` table (and the `/nfts` API) only
receives rows whose contract is `contract_type = Nft`, and **all 24 recovered contracts
are currently `Other`/`NULL` on prod** (only 1 contract chain-wide is `Nft`). So API
visibility needs, in order: **0296 (parser → pending) → `fix/0283` deployed + reclassify
run (`Other`→`Nft`, promote pending→hot) → raw-S3 backfill** for historically-dropped
rows. 0296 is upstream — promotion reads only from `*_pending`, which the parser never
wrote for these shapes (proof: the lone `contract_type=2` contract `CBBVYBTC` emits only
Shape-B events, 0 rows in every NFT table). The live classifier _would_ verdict these
`Nft` (they export `owner_of`/`token_uri`) — the `Other`/`NULL` is stale — but this PR
does not run reclassify.

**Option B (done 2026-06-17):** exact mutually-exclusive counts measured — chain-wide
36/14/20/2 = 72 (38 reach pending, 34 dropped = 20 vec + 14 map, 1 classified, 0 hot);
owner_of victims 13 map / 5 vec / 1 mixed = 19. Corrected the earlier "vec marginal"
claim — the `owner_of` lens had hidden the ERC-721 ports.

## Design Decisions

### From Plan

1. **Recover the parked stash** (Shape A scalar + Shape B packed-vec + tripwire +
   CAP-67 scval test) as the base — the task's Step 1.
2. **Tripwire over silent drop** — `tracing::warn!` on symbol-matched-but-unparsed so
   unknown future shapes surface instead of vanishing.

### Emerged

3. **Map-data reprioritised DEFERRED → first-class fix.** Prod + code-blind research
   showed `map{token_id}` is the canonical OZ/SEP-50 shape (the dominant real-NFT
   drop), not the "rare / maybe non-NFT" the original Context assumed. Implemented as
   Shape C (addresses in topics, token_id in the data map).
4. **`consecutive_mint` range expansion added** (not in original scope) — 8 mainnet
   contracts emit it (OZ Consecutive / EIP-2309); one event → N mints. Deferring it
   would silently under-count a whole collection's supply.
5. **Fungible-map disambiguation guard** (`is_fungible_map`) — the `map` shape is shared
   with CAP-67/SAC fungible (`map{amount,to_muxed_id}`, ~148M events); NFT iff the map
   carries `token_id`. Also suppresses the tripwire for that fungible firehose.
6. **`MAX_CONSECUTIVE_RANGE = 65_535`** — bound range-expansion of untrusted on-chain
   data against an alloc/CPU DoS in the Lambda indexer (aligned with the downstream i16
   event-order cap). Over-cap / inverted ranges → drop + tripwire.
7. **token_id emitted `{"type":"u64",…}` for consecutive** — values widened to u64;
   `token_id_to_string` stringifies regardless, so identity keys match the scalar path.
8. **Real-mainnet-XDR regression tests** — pinned actual on-chain bytes/values (Bachini
   vec, CARTUL5A map, CAKSC7JH consecutive) over synthetic-only shapes, given this
   module's history of measurement-evasive silent drops (0118 Patch C).
9. **Verifier scripts kept OUT of the repo** — the throwaway Node RPC scripts were moved
   to `.trash`; their evidence is preserved in the real-XDR tests + this README + the
   note's §Per-contract verification (avoids an unprecedented node toolchain in a
   Rust+lore tree — per review).

## Issues Encountered

- **Stash recovery**: the parked stash also carried unrelated `.tmp-rpc-0283/*` + lore
  READMEs; extracted ONLY `crates/xdr-parser/src/{nft.rs,scval.rs}` (base `1d421ba5`,
  unchanged on develop → clean checkout). Stash dropped after recovery (`5de34c69`,
  recoverable via reflog).
- **CH read quota**: full-table `soroban_events` scans exhausted the `dev_read`
  50B-rows/window quota mid-investigation; exact counts (option B) re-run after reset.
- **`map` false-positive**: the parse-time `token_id`-key heuristic flags 1 non-NFT
  (`CBCTZAZJ…`, a mint/withdraw contract) among the 16 map minters → 23/24 genuine NFTs.
  Contained: the WASM classifier verdicts it `Other`, so it stays in `nfts_pending` and
  never reaches hot. By design, the classifier is the authoritative NFT gate.
- **No modified tests**: the pre-existing `map_data_transfer_is_deferred_and_dropped`
  test still passes — it uses a fully-packed map (addrs in data) which Shape C (addrs in
  topics) correctly does not handle; still tripwired.

## Future Work

NFTs reach the user-facing `nfts` table only after these follow-ups — spawn as backlog
tasks **on develop** (project convention: never new task files on a feature branch):

1. **Deploy + run `fix/0283` reclassify on prod** — flips the 23 mislabeled
   `Other`/`NULL` NFT contracts to `Nft` (classifier matches `owner_of`/`token_uri`),
   promoting their pending rows to hot. Tracked under 0283 (the merged classification fix).
2. **Raw-S3 backfill re-parse** — the parser fix is parse-time only; historically-dropped
   map/vec/consecutive NFTs need a re-parse to materialize their `nfts_pending` rows
   (0296 Step 3; ops).
3. **Tail shapes** — `consecutive_range` is u32/u64-only (wider → tripwire); a non-`token_id`
   id-key map would tripwire. Both fail-safe; extend when the tripwire surfaces real cases.
4. **(minor, pre-existing)** `looks_like_token_id` accepts `address`/`bool`/`bytes` scalars
   as token_id (Shape A/B) — gated downstream by the classifier; tighten in a follow-up.
