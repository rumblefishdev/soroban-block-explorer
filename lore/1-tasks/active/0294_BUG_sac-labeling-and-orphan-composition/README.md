---
id: '0294'
title: 'BUG: SAC labeling + orphan composition — un-deployed SACs mislabeled is_sac=false; soroban_contracts registry pollution'
type: BUG
status: active
related_adr: []
related_tasks: ['0283', '0221', '0218', '0259']
tags:
  [
    clickhouse,
    sac,
    contract-classification,
    orphans,
    layer-data,
    priority-low,
    effort-medium,
  ]
links: []
history:
  - date: 2026-06-16
    status: backlog
    who: karolkow
    note: >
      Spawned from 0283 future work. Bundles the deploy-linkage "orphan"
      problem (#1) with the SAC-skeleton root-derivation problem (#6) — a
      2026-06-16 chain-validated deep-dive proved they CONVERGE on the same
      SAC-handling gap. DB-gated.
  - date: 2026-06-18
    status: active
    who: karolkow
    note: >
      Promoted to active. Red-team + blue-team chain verification (CAP-0067
      spec, independent SHA256 SAC-id derivation, live mainnet getEvents, prod
      ClickHouse re-count) CONFIRMED the core thesis: un-deployed SACs surfaced
      via CAP-67 unified events are the dominant orphan cause; numbers reproduce
      on prod today (4,310 orphans / 51,571,026 pending). 283's wasm_hash-JOIN
      rebuild cannot touch these (wasm_hash NULL). Deferred to implementation:
      skeleton counts are stale (294,963 was total-SAC, not the skeleton subset;
      current ~307k split across deployed_at NULL + =0 sentinel — the NULL-only
      predicate misses the =0 rows).
---

# BUG: SAC labeling + orphan composition

## Summary

`soroban_contracts` holds ~4,310 non-SAC "orphan" rows with NO deploy
(`deployed_at_ledger` NULL) and NULL `wasm_hash`, yet they emit events. A
2026-06-16 chain-validated deep-dive (blue+red, live mainnet) proved the
**dominant cause is un-deployed SACs surfaced via CAP-67 unified events**, not
mis-parsed WASM deploys. These are SACs mislabeled `is_sac=false` and are the
same SAC-handling story as the 294,963 forward-derived SAC-skeleton rows that
pollute `/v1/contracts`. This task sizes the orphan composition, fixes the SAC
labeling at its root, and routes the SAC routing-verdict out of the public
registry.

## Context

Spawned from **0283**. Orphans block NFT reclassification (no wasm link → no
`contract_type` → `nfts_pending` rows never promote), but only ~1.5% are real
NFTs and most are un-enrichable (66% absent from live state). The
SAC-skeleton (0218/0221) writes `is_sac=true` placeholders for every observed
classic asset's deterministic SAC `contract_id`; these byte-identical
placeholders pollute the public contract registry but are load-bearing as the
G9 routing-verdict cache for pre-window SACs.

### Chain-validated findings (2026-06-16)

Orphans split into **three buckets** (exact mix is DB-gated — run the split
query first, see [notes/G-orphan-split-queries.md](notes/G-orphan-split-queries.md)):

- **(a) DOMINANT — un-deployed SACs via CAP-67.** A classic asset's
  transfer/mint emits under its deterministic SAC `contract_id` even when no SAC
  was deployed. **148/148 sampled absent SAC-event emitters had
  `contract_id == Asset.contractId(mainnet)`** (exact cryptographic match).
  Root: the SAC-override path misses classic-**payment**-only events because
  `detect_classic_credit_assets` reads trustline changes only (`state.rs:918`).
  Fix = SAC-derivation (mark `is_sac=true`, `contract_type=Token`; wasm_hash
  legitimately NULL) — NOT a wasm link.
- **(b) phantom diagnostic-caller stubs.** PG path stubbed sub-call callers
  incl. failed/reverted (`write.rs:693`; CH path now skips diagnostic). Mega-
  offender `CDP5RUMSC7YJ…` (4.86M rows) is almost certainly a high-fanout
  caller, not an NFT emitter. Fix = truncate.
- **(c) minority — real WASM deploys mis-extracted.** Recoverable from
  `CreateContract` op-args (executable+preimage, dropped today at
  `stage.rs:1491`) or S3 re-parse.

**`parse_error`-deploy-tx candidate REFUTED:** a parse_error tx is `continue`d
before any extraction (`process.rs:297`) → produces NO rows.

### DB-confirmed (2026-06-16, prod CH via `chq`)

- **4,310 orphans; ALL 4,310 have `nfts_pending` rows, holding 51,571,026
  pending rows** (the bulk of pending).
- **Orphan event-signature profile** (the SAC smoking gun): `transfer` 344.6M
  (4,304 orphans), **`clawback` 1.78M (213)**, **`set_authorized` 1.19M (227)**,
  `burn` 1.05M (3,147), `mint` 298k (2,329). `clawback` and `set_authorized` are
  **SAC-only** classic-asset-control events (a custom Soroban NFT never emits
  them) → DB-confirms the un-deployed-SAC dominance, converging with the
  chain-probe 148/148 cryptographic match.
- **`soroban_contracts.name` = 0 of 424,220** (G5 names-off-ledger confirmed →
  enrichment, task 0297).
- **Q4: 0 of 4,310 orphans are in `assets`** (asset_type 2/3) → the SAC-derivation
  gap is DB-CONFIRMED — none were ever linked to an asset row.
- **Top offender `CDP5RUMSC7YJ76IE2QJHXIHLSYZSIDCMJMZUPFHCY34Y2AVGCRTPJZOV`
  (4,924,334 pending ≈ 10% of all orphan pending): CRYPTOGRAPHICALLY PROVEN an
  un-deployed SAC.** It EMITS 11.4M `transfer` + 44 `burn` (a real emitter, NOT a
  phantom-caller — refutes the red-team guess). A decoded transfer:
  `topics=[transfer, from, to, String("WGUARDIAN:GBYBVWOO…GUARD")]`,
  `data=i128(8441727124)` → a CAP-67 SAC transfer (amount, not a token_id), so its
  4.92M `nfts_pending` rows are FALSE POSITIVES (i128 amount mis-read as token_id).
  `Asset("WGUARDIAN","GBYBVWOO…GUARD").contractId(mainnet)` **==** the orphan's
  `contract_id`, exactly.
- **Implementation insight:** the asset `CODE:ISSUER` rides in the SAC event's
  **topic[3]** (String) → SAC-derivation can read it straight from the event
  (extend `detect_classic_credit_assets` / the SAC-override to these
  payment/transfer events), mark `is_sac=true`; the false-positive pending rows
  then drop out.
- **Phantom-caller magnitude ANSWERED (cheaply, via Q1):** `transfer` is emitted
  by **4,304 of 4,310 orphans** → at most ~6 are non-emitters (potential
  phantoms). The phantom bucket (b) is **negligible**; the bulk are real
  emitters → SAC (dominant) or WASM (minority). This refutes the red-team's
  "phantom-dominant top offender" — `CDP5RUMSC7YJ…` is a proven SAC emitter.
- **Full per-orphan crypto-split (A3): byte-quota-blocked until 13:00**
  (`read_bytes` 100GB/h hit while reading the heavy `topics_xdr` column for one
  event per orphan). Method is proven (it nailed the top offender); re-run the
  `LIMIT 1 BY contract_id` → `JSONExtractString(topics_xdr, 4, 'value')` →
  Rust/JS SAC-derive after reset to size the exact SAC-vs-WASM-minority split.

## Implementation Plan

### Step 1 — DB split (sizes every fix before investing)

Run the decisive SPLIT (notes/): for each orphan join `events`, decode the
modal event's asset topic → `Asset(code,issuer).contractId(passphrase)`, test
equality → SAC share; non-matching ∩ has a `CreateContract` op → WASM share;
neither → phantom. (CH can't SHA256/XDR — export + derive in Rust via
`xdr_parser::sac::derive_sac_contract_id`.)

### Step 2 — SAC-derivation (bucket a, dominant)

Extend SAC-override derivation to classic-**payment** events (not just trustline
changes). Mark mislabeled orphans `is_sac=true`, `contract_type=Token`.

### Step 3 — SAC-skeleton root (registry pollution)

Move the SAC routing-verdict out of `soroban_contracts` into a side-table; keep
the public registry to real deployed contracts. **HARD CONSTRAINT:** the
side-table MUST carry the `Token` verdict for **pre-window** SACs (the
skeleton's current G9-routing job) or the 0221 event-leak returns —
**re-validate 0221 as the acceptance gate.**

### Step 4 — phantom bucket

TRUNCATE decision for the diagnostic-caller stubs (bucket b).

### Step 5 — WASM minority forward-fix (bucket c)

Persist `(contract_id, wasm_hash)` from `CreateContract` op-args at ingest so
new deploys can never become orphans; existing → re-parse.

## Acceptance Criteria

> **Re-scoped 2026-06-23 (PR #272).** Deep-dives reframed the original 5-step plan.
> The shipped scope is the **false-NFT fix at its source** (a detection-stage SAC gate)
>
> - the batch history-repair; the registry-model half, the classifier, and the prod run
>   are spawned to dedicated follow-ups (0323, 0317, 0315/0303, 0295) rather than bundled.

- [x] Orphan composition sized (SAC / phantom / WASM) — 5,558 crypto-confirmed SAC, 0
      rejected, phantom = 1, WASM = 0 (read-only prod, validated)
- [x] Un-deployed-SAC false NFTs stopped at source — **detection-stage gate** in
      `detect_nft_events` (supersedes the original "is_sac=true relabel" framing; the
      batch `sac-orphan-relabel` repairs history). Prod flip → **0315 / 0303**.
- [→] `/v1/contracts` no longer polluted — **un-deployed SAC = asset, not contract**;
  spawned to **0323** (writer skip + LEFT-join; not a side-table). NOT in this PR.
- [x] 0221 event-leak re-validated — the detection gate prevents the SAC→pending leak at
      source (test `sac_transfer_is_gated_out_not_an_nft`); side-table verdict carrying
      no longer needed.
- [→] WASM-orphan forward-fix + phantom — phantom = 1 (resolved, empty); WASM-orphan
  prevention is a separate latent gap (0 today) → **0295**.
