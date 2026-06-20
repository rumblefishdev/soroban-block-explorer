---
id: '0308'
title: 'Custom-ABI NFT coverage: parser shapes + classifier verdicts + never-silently-drop'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0306', '0296', '0283']
tags:
  [
    'nft',
    'xdr-parser',
    'classifier',
    'backfill',
    'phase-future',
    'effort-medium',
    'priority-medium',
  ]
links:
  - 'https://api.stellar.expert/explorer/public/contract/CBMKSLJL6UFPKIE76ASSEKSP4H7ZWL3PCX6NGMWBARR5RH2GHB7U5QMJ'
history:
  - date: 2026-06-19
    status: backlog
    who: karolkow
    note: 'Spawned from an nft-reparse tripwire during 0306. Scope set via empirical CH census + two adversarial deep-researches (SEP-50/OZ docs + on-chain RPC). All claims chain-verified.'
---

# Custom-ABI NFT coverage

## Summary

Real on-chain NFT collections with **non-standard (custom) ABIs** are missed by our pipeline on
two independent layers: the **parser** drops event shapes it does not recognise, and the **WASM
classifier** labels the contract `Other` so its rows never get promoted to the hot `nfts` table.
This surfaced during 0306 (Staszek's reparse logged tripwire WARNs). This task makes the parser +
classifier cover the custom families found on mainnet, and — crucially — turns the silent drop into
a **never-silently-drop** path so future custom ABIs surface instead of vanishing.

## Context — what we actually know (chain-verified, not from our code/logs)

**The documented universe is ALREADY handled (task 0296).** Per SEP-50 + OpenZeppelin source, a
Soroban NFT emits `transfer`/`mint`/`approve`/`approve_for_all` (+ `burn`, + `consecutive_mint`
range), in four data encodings — Shape A scalar, Shape B packed-vec, Shape C `map{token_id}`,
consecutive-range. `nft.rs` already parses all four. token_id is variable-width (u32/u64/i128/bytes)
— store as string.

**The misses are UNDOCUMENTED custom ABIs** — they exist in NO SEP / OZ / library, and the deep-
research confirmed they **cannot be enumerated from docs**, only discovered empirically on-chain.
So our CH census is the source of truth for the current miss-set.

**Census result (authoritative, clean SQL):**

- ~50 contracts / 34 wasm: working (classified `Nft`, in hot `nfts`).
- **Parser-layer miss** — classified `Nft` but 0 rows because their event shapes are unhandled
  (Staszek's "14 with events, 0 tokens").
- **Classifier-layer miss** — real NFTs (have `mint` + `token_id`, no `decimals`/`allowance`) stuck
  at `Other`, so never promoted. The `nft_reclassify` run already proved this is the dominant gap.

**Two example families:**

- **8888-style (clean):** `CBMKSLJL…` ("8888 SKELETONS") + ~10 siblings. Inverted shape — `transfer`
  carries `[Symbol, U32 token_id]` in topics and `Address(to)` in data (token_id in TOPIC, single
  address = recipient, NO `from`). Proven `data=to` 8/8 chain-on-chain. Main mint event is
  `bulk_mint` (`[bulk_mint, Address to]` + `data=vec[u32…]`), which is not even a recognised symbol.
- **`minted`-style (messy):** ~14 NFT-ish contracts use symbol `minted` with token_id as **bytes**
  (a hash) in a topic and recipients in a data `vec` — a totally different encoding. Proof that the
  custom tail is heterogeneous and unbounded.

## Implementation Plan

### Step 1 — Parser: add the custom shapes (`crates/xdr-parser/src/nft.rs`)

Clean tier (high value, well understood — the 8888 family):

- inverted `transfer`: topics `[transfer, U32 token_id]`, data `Address(to)` → `NftEvent{token_id, to:Some, from:None}`.
- `bulk_mint` symbol: topics `[bulk_mint, Address to]`, data `vec[u32…]` → one mint per id. Guards: handle empty `vec[]` (no-op), cap element count (mirror `MAX_CONSECUTIVE_RANGE`, DoS guard).
- inverted `mint`: topics `[mint, U32 token_id]`, data `Address(to)`.

Messy tier (verify shape per family before adding): `minted` (bytes token_id + vec recipients),
`identity_minted`, `transfer[2,vec]`, etc. — handle the clearly-NFT ones; the rest ride the tripwire.

### Step 2 — Never-silently-drop (the part that ends the re-run cycle)

- Route EVERY unrecognised mint/transfer/burn-ish symbol **and** unparsed shape through the existing
  `maybe_tripwire` (today `bulk_mint`/`minted` hit the catch-all `_ => continue` and vanish with NO
  warning). After this, an un-handled NFT candidate is always **surfaced**, never silently lost.
- Optional (small): make the tripwire durable/aggregable (count by contract/symbol) so new families
  are noticed proactively, not by reading worker logs. NOT a new pending table — `nfts_pending` stays
  the holding area for PARSED candidates; this is only for the not-yet-parsable ones.

### Step 3 — Classifier (`crates/xdr-parser/src/classification.rs`)

- Generalise the `Nft` rule to: **has `mint` + a `token_id` concept + NO fungible markers
  (`decimals`/`allowance`/`total_supply`)**. On the current census this catches the custom families
  with **0 fungible false-positives**. Keep it a COMBINATION (avoid single-name over-match); the WASM
  classifier remains the authoritative gate (the data-map-key heuristic has rare FPs).
- `[verify first]` confirm `classify_contract_from_wasm_spec` can read what the rule needs, and that
  `contract_type_rebuild` (0283) actually re-runs over existing contracts (not only new uploads).

### Step 4 — Ops sequence (already the 0306 order; correctness depends on it)

Candidates FIRST, classify LAST: fixed parser + **reparse** (fills `nfts_pending`) →
`contract_type_rebuild` / 0283 (sets `Nft`) → `nft_reclassify` (promotes pending → hot). Running
reclassify before candidates exist cannot promote a contract that has no pending rows.

## Acceptance Criteria

- [ ] Parser emits the clean-tier shapes (inverted `transfer`/`mint`, `bulk_mint`) with real on-chain
      XDR regression fixtures (CBMKS family). Empty-vec + element-cap guards covered.
- [ ] No silent drop: every unrecognised candidate symbol/shape tripwires (no bare `_ => continue`).
- [ ] Classifier returns `Nft` for the census miss-set across all wasm versions; **0 fungible false-
      positives** on the full contract population.
- [ ] After reparse → rebuild → reclassify, the missed families appear in hot `nfts`; ownership for a
      sampled set matches on-chain `get_token_info`.
- [ ] **Docs updated** — changes XDR parsing + classification; update `docs/architecture/**` per
      [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [ ] **API types regenerated** — `N/A` (no `crates/api/**` / `Cargo.*` / `libs/api-types/**` change).

## Completeness — honest statement

After this task + the 0306 ops sequence, **current mainnet NFTs are covered** (the census enumerated
every shape that exists today). It does **not** prove "never miss again": the deep-research showed
custom ABIs are heterogeneous and unbounded on a permissionless chain (`minted` alone uses a wholly
different encoding). The **never-silently-drop tripwire (Step 2) is what breaks the re-run cycle** —
new families land in a visible queue, drained incrementally, with no from-scratch reparse. A
permanent, provable guarantee would need the deferred fundamental redesign (typed SEP-48 event specs

- monitored-UNKNOWN total-function classifier — see auto-memory `reference-soroban-classification-seps`).

## Notes

- **Relation to other tasks:** [0296](../archive/0296_BUG_nft-event-extraction-completeness/README.md)
  added the documented shapes; [0283](#) is `contract_type_rebuild`; [0306](0306_OPS_nft-surfacing-enrichment-prod-pipeline.md)
  is the ops pipeline that runs reparse → rebuild → reclassify. This task is the upstream coverage fix.
- **Devils-advocate residual risks:** classifier FP rule must be tested on the whole population;
  confirm reclassify re-runs over existing contracts; `data=to` proven on 1 of 4 wasm versions
  (structure uniform — close with a `get_token_info` cross-check on the other versions).
- **Verification trail:** full record + numbers in auto-memory `project-custom-abi-nft-class-missed`.
