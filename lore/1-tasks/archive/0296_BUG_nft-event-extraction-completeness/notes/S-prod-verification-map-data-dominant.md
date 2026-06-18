---
title: 'NFT event-shape: prod + code-blind standards + live-RPC verification; map{token_id} is the canonical drop'
type: synthesis
status: mature
spawns: []
tags:
  [
    nft,
    xdr-parser,
    extraction-completeness,
    map-data,
    prod-verified,
    rpc-verified,
    code-blind,
    standards,
    layer-data,
  ]
links:
  - crates/xdr-parser/src/nft.rs
history:
  - date: 2026-06-16
    status: developing
    who: karolkow
    note: >
      Prod ClickHouse verification (chq, read-only). Found map{token_id} is the
      dominant silent-drop shape among NFT contracts, not Shape B.
  - date: 2026-06-17
    status: mature
    who: karolkow
    note: >
      Expanded after merging fix/0283: added code-blind standards model (3 data
      encodings + consecutive_mint + fungible disambiguation), live-RPC raw-XDR
      verification, and a two-lens reconciliation (134 owner_of cohort
      vs ~72-80 chain-wide minters). Confirms map{token_id} = canonical OZ/SEP-50
      shape our parser drops. Option B done: exact mutually-exclusive counts folded
      in (chain-wide 36 scalar / 14 map / 20 vec / 2 mixed = 72; 38 reach pending,
      34 dropped = 20 vec + 14 map). Corrected an earlier overstatement that
      vec/Shape B was marginal — chain-wide it is the larger dropped group (20).
---

# NFT event-shape — three-layer verification

> Synthesis, 2026-06-16 → 2026-06-17, karolkow (with Claude). Status: mature.
> Verified at THREE independent layers, deliberately NOT trusting our own code:
> (1) prod ClickHouse (our captured chain, `chq`, read-only), (2) code-blind
> standards + ecosystem research (SEP-41/50, CAP-46/67, OpenZeppelin source),
> (3) **live Stellar RPC** decoding raw event XDR — pinned as the `detect_real_mainnet_*`
> Rust tests in `crates/xdr-parser/src/nft.rs`.
> Context: `fix/0283` (classification fix) is merged into this branch; the parser
> fix is parked in `stash@{0}` (Shape B + CAP-67 test, NOT map-data).

## Bottom line

1. The parser (`detect_nft_events`) silently drops every NFT event whose `data`
   is not a bare scalar — no log, no metric. **Confirmed in code + on prod + on
   live chain.**
2. The parser drops **34 of 72** NFT-minting contracts chain-wide: **20 use
   `data = vec[…]`** (ERC-721 ports — the parked-stash / "Shape B" fix) and
   **14 use `data = map{"token_id"}`** — the **canonical OpenZeppelin / SEP-50
   shape** (the `#[contractevent]` macro defaults to map-by-field-name), which
   0296 wrongly DEFERS as "rare / maybe non-NFT". (Earlier I called vec marginal —
   wrong: the `owner_of` lens hid the ports. Both matter; map is the standard one.)
3. **0296 ALONE moves these events from silently-dropped → `nfts_pending`, NOT to
   the user-facing hot `nfts` table / `/nfts` API.** Promotion to hot only `SELECT`s
   from pending `WHERE contract_type = Nft`; all 24 recovered contracts are currently
   `Other`/`NULL` on prod (stale — they export `owner_of`/`token_uri`, so reclassify
   will flip them). API visibility needs, in order: 0296 (→ pending) → `fix/0283`
   reclassify run (→ hot) → raw-S3 backfill (historical). 0296 is upstream of 0283;
   this PR does not run reclassify. (Sharpened per devil's-advocate review.)
4. A map handler MUST disambiguate on the map KEY: `token_id` = NFT vs
   `amount`/`to_muxed_id` = fungible. Chain-wide there are 147.9M fungible
   `map{amount,to_muxed_id}` mints vs 890 NFT `map{token_id}` mints — naive "handle
   map" would mis-ingest ~5,580 fungible contracts as NFTs.

## Layer 2 — code-blind standards model (what a CORRECT parser must handle)

Sources (no repo): SEP-41, SEP-50, CAP-46-6, CAP-67, OpenZeppelin
`stellar-contracts` `non_fungible/mod.rs`, soroban-sdk `#[contractevent]` docs.

Canonical NFT event = `topics = [Symbol(name), addresses…]`, `data` carries the
`token_id`. The identifier appears in **three legitimate `data` encodings**:

| `data` encoding | who emits it | standard? | our parser |
| --- | --- | --- | --- |
| `map{"token_id": uN}` | OpenZeppelin `stellar-contracts` (the de-facto NFT lib) + SEP-50; `#[contractevent]` **default** | **canonical** | ❌ dropped |
| bare scalar `u32/u64` | contracts using `data_format="single-value"` | variant | ✅ handled |
| `vec[addr…, token_id]` | hand-rolled ERC-721 ports (e.g. JamesBachini) | non-standard, real | ❌ dropped (stash handles) |

Other facts a correct parser needs:
- **`consecutive_mint`** (OZ Consecutive / EIP-2309): `topics=[consecutive_mint, to]`,
  `data` = a `[from_id, to_id]` RANGE = many tokens in one event. Present on prod
  (8 contracts). Handled by no current plan.
- **token_id width is not fixed** — u32 typical, SEP-50 trends u256. Store as string.
- **Disambiguation NFT vs fungible** (same symbols!): fungible `data` = `i128`
  amount, or `map{amount, to_muxed_id}`, or a 4th `sep0011_asset:String` topic (SAC).
  NFT `data` = an integer id (often in `map{token_id}`).
- `approve`/`set_token_royalty` put `token_id` in a TOPIC; `approve_for_all` carries
  no token_id — recognize-but-exclude from ownership accounting.

## Layer 3 — live RPC verification (raw XDR, independent of our DB)

Method: `getEvents` + `getLedgerEntries` against `mainnet.sorobanrpc.com` (tip ledger
~63,062,500, protocol 26), decoding each event's raw topic/data XDR with
`@stellar/stellar-sdk` (pinned permanently as the `detect_real_mainnet_*` Rust tests).
Results:

- **RPC `getEvents` retention = 7 days.** Historical NFT mints (Bachini @ledger
  ~54.6M, ~1.3 yr) are unreachable by events — only via instance probe or our
  captured `soroban_events`. This is why 0296's 1-day sweep saw "0 Shape B / map
  only non-NFT": the NFT tail is historical.
- **Instance existence** (`getLedgerEntries`, retention-independent): Bachini,
  the labeled-NFT, and sample map-NFTs all exist with a wasm hash. Bachini's wasm
  `c5e2d06e1d…724a` matches stellar.expert — independent cross-confirmation.
- **Live canonical NFT shape**, decoded from raw XDR within the window:
  - `CARTUL5A…` `mint` 2-topic, `data = map{token_id}` → `token_id 133`.
  - `CCHHGIOB…` `mint` 2-topic, `data = map{token_id}` → `token_id 93` (a domain NFT;
    also emits `approve_for_all map{operator, live_until_ledger}` = SEP-50 shape).
  - Same contracts emit `collect`/`increase_liquidity` with `map{…, token_id}`
    (Uniswap-v3-style position NFTs).
- **Fungible map dominance** (contrast): recent mints are ~100% fungible —
  `i128` amount or `map{amount, to_muxed_id}` (e.g. `to_muxed_id:"staking reward…"`).
  Proves the map shape is shared → disambiguation is mandatory.

## The numbers — two lenses (they answer different questions)

Mixing these is what made an earlier funnel "not add up". They are distinct
populations by definition.

### Lens A — the 134 cohort (sums exactly)

Population: `soroban_contracts` ⋈ `wasm_interface_metadata` on `wasm_hash`, where
`metadata LIKE '%owner_of%'` (contracts with an NFT interface).

| bucket | n | source (table.field / filter) | bug/fix |
| --- | --- | --- | --- |
| total | **134** | the join above | — |
| inactive | 82 | no mint/transfer/burn rows in `soroban_events` | not a bug |
| Shape A → pending | 34 | present in `nfts_pending`/`nft_ownership_pending` | 0283 only |
| parser dropped | 18 | emit in `soroban_events`, 0 rows in both `*_pending` | **0296** |

82 + 34 + 18 = **134** ✓ (strict-case match; case-insensitive `lower(signature)`
catches a 19th emitter). Dropped victims by shape, **exact mutually-exclusive**
(`soroban_events.data_xdr`): **13 `map{token_id}`-only + 5 `vec`-only + 1 mixed = 19**.
Classification axis (`soroban_contracts.contract_type`): 1 = Nft (2), 133 = Other (1).
Within this `owner_of` cohort map dominates (13 vs 5) — but that is a filter artifact
(OZ NFTs expose `owner_of`; hand-rolled vec ports usually do not — see Lens B).

### Lens B — chain-wide NFT minters (unbiased, broader)

`soroban_events`, `lower(signature)='mint'`, grouped by
`JSONExtractString(data_xdr,'type')` (and map key):

| `data` | events | contracts | class | parser |
| --- | --- | --- | --- | --- |
| `i128` | 256,454,593 | 170,889 | fungible (amount) | n/a |
| `map` key `amount`/`to_muxed_id` | 147,942,815 | 5,580 | fungible (SAC muxed) | n/a |
| `map` key `token_id` | 890 | **16** | **NFT** | ❌ dropped |
| `u32`/`u64` scalar | 11,166 | **37** | **NFT** | ✅ handled |
| `vec` | 362 | **20** | **NFT** packed | ❌ dropped |
| `address`/`void` | 167 | 3 | odd | ❌ |

→ NFT-minting contracts (token_id-bearing, dedup): **72**, exact mutually-exclusive:
**36 scalar-only + 14 map-only + 20 vec-only + 2 mixed = 72** (the table above is
any-occurrence, so its 37/16 run slightly higher). Pipeline of the 72: **38 reach
pending** (36 scalar-only + 2 mixed — their scalar mints parse) · **34 dropped
entirely = 20 vec-only + 14 map-only** · 1 classified Nft · 0 in hot. Plus
`signature='consecutive_mint'`: 23 events / **8** contracts → **~80** NFT contracts.

### Global prod state

`contract_type` (`soroban_contracts FINAL`): 0/SAC=311,153 · 1/Other=107,457 ·
2/Nft=1 · 3/Fungible=2 · NULL=5,607. Hot `nfts`=0, `nft_ownership`=0. Pending
`nfts_pending`=61,564,409, `nft_ownership_pending`=143,172,979.

## Pipeline dependency (post-0283-merge)

`fix/0283` (merged) fixes classification: cross-ledger verdict via
`domain/classification_cache.rs` (G1 deploy + G9 routing) and the batch
`backfill-runner contract-type-rebuild`. Done in code, **not deployed** (prod
still shows 1 Nft). But promotion (`nft_reclassify.rs`:
`INSERT INTO {hot} SELECT * FROM {pending} WHERE contract_type=2`) reads ONLY from
pending; pending rows come ONLY from `detect_nft_events`. So for map/vec/consecutive
NFTs the parser drops → no pending row → 0283 promotes nothing. Proof on prod:
`CBBVYBTC…` is the lone `contract_type=2` (same-ledger fluke), emits 6 events all
1-topic/vec (Shape B), and has 0 rows in every NFT table.

## Recommendation (scope correction for 0296)

1. **Apply the parked stash** → recovers the 20 `vec` (Shape B) ports + adds the
   tripwire. This is the LARGER dropped group (20 contracts), not marginal.
2. **Also handle `map{token_id}`** (currently deferred) — 14 contracts, the
   canonical OZ/SEP-50 shape; trivial, key is literally `token_id`. Stash + map
   together recover all 34 dropped.
3. **Add `consecutive_mint`** (range expansion) — new gap, no current coverage.
4. **Disambiguation guard is mandatory**: NFT iff data carries `token_id` (not
   `amount`/`to_muxed_id`, no `sep0011_asset` topic). Else ~5,580 fungible
   contracts get mis-ingested.
5. CAP-67/scval address rendering is already correct (`stellar-xdr 26`) → test-only.
6. Backfill (raw-S3 re-parse) still required to materialize historical drops.

## Caveats / not proven

- The parse-time `token_id`-key heuristic is permissive and CAN false-positive a
  non-NFT: of the 16 `map{token_id}` minters, RPC-verified **15 are NFTs**
  (`owner_of`/`token_uri` in wasm, sequential token_id), but **1 — `CBCTZAZJ…` — is
  NOT** (interface `__constructor/mint/withdraw`, no NFT fns). Harmless: the downstream
  WASM classifier verdicts it `Other` → it stays in `nfts_pending` quarantine and never
  reaches the hot `nfts` table. (`CARTUL5A` is a genuine position-NFT, NFT + AMM — still
  a real ERC-721 token_id.) `owner_of` is itself a substring filter (impure: caught ≥1
  LP, ≥1 marketplace), so the per-contract wasm-fn check above is the authoritative one.
- "Canonical" precision: `map{token_id}` is canonical via the **OpenZeppelin reference
  impl + the soroban-sdk `#[contractevent]` map-by-field default**. SEP-50's prose
  describes the data as an ordered list (a `TokenID` element), not literally a map — so
  the map encoding is OZ/SDK-canonical, with SEP-50 as the NFT interface standard.
- Exact mutually-exclusive counts now measured (option B done, 2026-06-17): chain-wide
  36/14/20/2 = 72; victims 13 map / 5 vec / 1 mixed = 19. The earlier "Shape B marginal
  (~2)" was an `owner_of`-filter artifact — chain-wide vec is 20 (ports usually lack
  `owner_of`).
- Lens A (134, interface) and Lens B (72, minters) differ by definition; the gap is
  exactly the ERC-721 vec ports with no `owner_of` (caught only by Lens B).

## Appendix — reproducible queries (`chq`, read-only)

```sql
-- contract_type census
SELECT contract_type, count() FROM soroban_contracts FINAL GROUP BY contract_type ORDER BY contract_type;
-- chain-wide mint shapes
SELECT JSONExtractString(data_xdr,'type') t, count(), uniqExact(contract_id) FROM soroban_events WHERE lower(signature)='mint' GROUP BY t ORDER BY 2 DESC;
-- map-mint key split (NFT vs fungible)
SELECT countIf(data_xdr LIKE '%"value":"token_id"%') nft, uniqExactIf(contract_id, data_xdr LIKE '%"value":"token_id"%') nft_ct,
       countIf(data_xdr LIKE '%"value":"amount"%') fung, count() total
FROM soroban_events WHERE lower(signature)='mint' AND JSONExtractString(data_xdr,'type')='map';
-- consecutive_mint presence
SELECT count(), uniqExact(contract_id) FROM soroban_events WHERE signature='consecutive_mint';
-- option B (DONE 2026-06-17): exact mutually-exclusive encoding → 36 scalar / 14 map / 20 vec / 2 mixed = 72
WITH m AS (SELECT contract_id AS cid,
    max(JSONExtractString(data_xdr,'type') IN ('u32','u64')) AS sc,
    max(JSONExtractString(data_xdr,'type')='map' AND data_xdr LIKE '%"value":"token_id"%') AS mp,
    max(JSONExtractString(data_xdr,'type')='vec') AS vc
  FROM soroban_events WHERE lower(signature)='mint'
    AND (JSONExtractString(data_xdr,'type') IN ('u32','u64','vec') OR (JSONExtractString(data_xdr,'type')='map' AND data_xdr LIKE '%"value":"token_id"%'))
  GROUP BY contract_id)
SELECT countIf(sc=1 AND mp=0 AND vc=0) only_scalar, countIf(mp=1 AND sc=0 AND vc=0) only_map,
       countIf(vc=1 AND sc=0 AND mp=0) only_vec, countIf((sc+mp+vc)>1) mixed, count() total FROM m;
-- pipeline of the 72 → reach_pending=38, classified_nft=1, in_hot=0 (replace SELECT with countIf(cid IN pending/contract_type=2/hot))
```

Implementation proof (permanent): the `detect_real_mainnet_*` Rust tests in
`crates/xdr-parser/src/nft.rs` run the real parser on real on-chain XDR/values. The RPC
method above (`getEvents`/`getLedgerEntries` + `@stellar/stellar-sdk` decode) is the
reproducible recipe; the throwaway verifier scripts were not committed.

### Per-contract verification (RPC, 2026-06-17) — preserves the (uncommitted) verifier output

Method: `getLedgerEntries` → fetch each contract's wasm → scan for NFT interface fns
(`owner_of`/`token_uri`/`approve_for_all`); `getEvents` (7-day window) → decode live
events' raw XDR. Result: **23 of 24 genuine NFTs**; the 1 false-positive is
quarantine-contained.

**map{token_id} cohort (16): 15 NFT + 1 non-NFT.** Live mints decoded from raw XDR:
`CARTUL5A` token_id 133, `CCHHGIOB` token_id 93 (domain NFT, emits `approve_for_all`),
`CADCRH6B` token_id 391, `CC2RSPG4` token_id 11.

- NFT (15, `owner_of`+`token_uri` in wasm): CADCRH6B, CAMOZBTH, CARTUL5A, CAVK536D,
  CBGPDCJI, CBQ5FHBA, CC2RSPG4, CC3TXXYT, CC5CQHSG, CCHHGIOB, CCIP47L5, CCSMX3YE,
  CCTPN4LR, CCYBGAFJ, CDN3HRO5.
- NOT NFT (1): **`CBCTZAZJBG5TZEC2WHHAD4J4JKPQCUNTMULVYMUYLDQMAXUAIQPOOMKI`** — interface
  `__constructor/mint/withdraw`, no NFT fns. token_id-key false-positive; classifier
  verdicts `Other` → stays in `nfts_pending`, never reaches hot.

**consecutive_mint cohort (8): all 8 NFT** (`owner_of`+`token_uri`+`approve_for_all`):
CAKSC7JH, CAWR2V6W, CB3MNEZR, CBNAIBGJ, CBTPRIK3, CCZZKCAM, CDDMH5FO, CDMHVSVG. Real
range decoded: `CAKSC7JH` `vec[4,149]` → 146 mints (pinned in the
`detect_real_mainnet_consecutive_mint_range` Rust test).
