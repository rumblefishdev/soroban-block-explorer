---
id: '0294'
title: 'BUG: SAC labeling + orphan composition — un-deployed SACs mislabeled is_sac=false; soroban_contracts registry pollution'
type: BUG
status: completed
related_adr: []
related_tasks: ['0283', '0221', '0218', '0259', '0303', '0307']
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
  - date: 2026-06-18
    status: completed
    who: karolkow
    note: >
      CLOSED - code delivered (mirrors the 0283 -> 0303 code/rollout split).
      Scope: (1) LIVE forward-fix `derive_sac_overrides_from_events` + shared
      `sac_override_from_event_topics` crypto-match gate, wired into the indexer
      parse path; (2) batch `backfill-runner sac-orphan-relabel` to relabel the
      ~5,607 historical orphans. Full crypto-split DONE (5,607/5,607 confirmed
      un-deployed SACs, 0 WASM, 0 phantom). 5-agent review (correctness + 2x
      simplify + devil + checklist) converged: correct / safe / senior /
      right-sized / consistent. 254 xdr-parser + 3 backfill tests green, clippy
      -D warnings clean. Architecture docs updated (ADR 0032). Step 3 (registry
      de-pollution) spun to 0307; the prod RUN + the relabel's prod OOM-query
      fix go to 0303 (scope updated there). Evidence:
      notes/S-devil-advocate-full-crypto-split.md + notes/S-review-synthesis.md.
---

# BUG: SAC labeling + orphan composition

## Summary

`soroban_contracts` holds **~5,607** non-SAC "orphan" rows with NO deploy
(`coalesce(deployed_at_ledger, 0) = 0`) and NULL `wasm_hash`, yet they emit
events. A 2026-06-18 devil's-advocate **FULL** crypto-split (live mainnet + prod
CH — [notes/S-devil-advocate-full-crypto-split.md](notes/S-devil-advocate-full-crypto-split.md))
proved the cause is un-deployed SACs **not merely "dominant" but TOTAL**:
4,304/4,304 transfer-emitters + 6/6 mint-emitters in the NULL set, and
1,199/1,199 in the `=0`-sentinel set, ALL match
`contract_id == Asset.contractId(mainnet)` exactly — **0 mismatch, 0 real-WASM,
0 phantom**. These are SACs mislabeled `is_sac=false`, the same SAC-handling
story as the SAC-skeleton rows that pollute `/v1/contracts`. **Mechanism is
mixed — direct SAC invocation (pre-P23, 68% of events) + CAP-67 (post-P23) — NOT
just CAP-67** (correction below). This task fixes the SAC labeling at its root
(crypto-match-gated) and routes the SAC routing-verdict out of the public
registry (0221-leak-gated).

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

### Devil's-advocate FULL crypto-split + corrections (2026-06-18)

Full split now DONE (supersedes "A3 byte-quota-blocked"); see
[notes/S-devil-advocate-full-crypto-split.md](notes/S-devil-advocate-full-crypto-split.md).

- **Cause is TOTAL, not "dominant":** 4,304/4,304 transfer + 6/6 mint emitters
  (NULL set) and 1,199/1,199 (`=0` sentinel set) crypto-match their SAC id. **0
  mismatch, 0 WASM, 0 phantom.** Deriver validated 3/3 (XLM/USDC/WGUARDIAN) +
  stellar.expert; orphans 404 (no instance), deployed SAC 200. Pending impact:
  51,571,026 / 61,564,409 (83.8%).
- **C2 — population is 5,607, not 4,310.** The `deployed_at_ledger IS NULL`-only
  predicate misses **1,297** identical `=0`-sentinel orphans. Use
  `coalesce(deployed_at_ledger, 0) = 0` everywhere.
- **C4 — mechanism mis-labelled.** P23 activated @ ledger 58,762,517
  (2025-09-03); **68% of orphan events + 97% of orphans are PRE-P23** → emitted
  via direct SAC host-function invocation (P20+), not CAP-67. Post-P23 adds
  CAP-67 unified events. The fix reads the asset from the event topic regardless
  of mechanism, so it is unaffected — but "via CAP-67" is wrong for the bulk.
- **C5 — no phantom bucket, no 0296 overlap.** ≤6 non-transfer orphans, all SAC
  → Step 4 dropped. 0296 (wasm NOT NULL) holds 359,751 pending; 0294 (wasm NULL)
  holds 51.5M — disjoint by the wasm axis.

## Implementation Plan

### Step 1 — DB split — DONE 2026-06-18 (re-confirm sizing with corrected predicate)

The split is complete (devil's-advocate note): **5,607/5,607 orphans are
crypto-confirmed un-deployed SACs — 0 WASM, 0 phantom.** Before migrating,
re-run sizing with the corrected predicate `coalesce(deployed_at_ledger, 0) = 0`
(the old NULL-only predicate undercounts by 1,297). Method that produced it:
export `transfer`/`mint` topic[3] (orphan-set as a SUBQUERY — never an inline
`IN(...)`, which OOMs at 3.73 GiB) → `Asset(code,issuer).contractId(mainnet)`
via `notes/devil/sac_derive.py` / `xdr_parser::sac::derive_sac_contract_id` →
test id equality.

### Step 2 — SAC-derivation (the whole population)

Extend SAC-override derivation to classic-**payment**/transfer events (not just
trustline changes): read the asset `CODE:ISSUER` from the SAC event's topic[3],
mark mislabeled orphans `is_sac=true`, `contract_type=Token`.

**C1 GUARDRAIL (Critical) — gate on the CRYPTOGRAPHIC MATCH, not event shape.**
The flip MUST require `emitter_id == derive_sac(topic[3])`. 370 real WASM
contracts emit `transfer` and **3 emit the exact 4-topic SAC shape**
`[sym, addr, addr, String("CODE:ISSUER")]`; a shape-only flip corrupts them (the
crypto-match rejects all 3). Make the match an explicit, **tested** precondition.
Write via the existing `wasm_uploaded_at_ledger = 0` SAC-override path so a
future real deploy auto-outversions the flip under RMT/FINAL.

### Step 3 — SAC-skeleton root (registry pollution)

Move the SAC routing-verdict out of `soroban_contracts` into a side-table; keep
the public registry to real deployed contracts. **HARD CONSTRAINT:** the
side-table MUST carry the `Token` verdict for **pre-window** SACs (the
skeleton's current G9-routing job) or the 0221 event-leak returns.

**C3 GUARDRAIL (High) — the re-leak hangs on ONE query.** The 0221 fix is
`query_contract_verdicts` (`crates/db-clickhouse/src/persist.rs:380-383`):
`… FROM soroban_contracts FINAL WHERE contract_type IN (0,2,3)`. All 311,153 SAC
rows carry `contract_type=0` (Token); moving the skeletons makes this query miss
the verdict → **instant re-leak**. Repoint/UNION it to the side-table (or keep a
verdict-only projection in `soroban_contracts`). **Acceptance gate (runnable):**
replay an event from a known un-deployed SAC, assert it does NOT land in
`nfts_pending`.

### Step 4 — phantom bucket — DROPPED (empty)

The full split found NO phantom bucket: ≤6 non-transfer orphans, all 6
crypto-confirmed SACs. No TRUNCATE needed.

### Step 5 — WASM minority forward-fix (bucket c)

Persist `(contract_id, wasm_hash)` from `CreateContract` op-args at ingest so
new deploys can never become orphans; existing → re-parse.

## Acceptance Criteria

- [x] Orphan composition sized — 5,607/5,607 crypto-confirmed SAC, 0 WASM, 0 phantom (2026-06-18)
- [x] Step 2 flip gated on crypto-match `emitter_id == derive_sac(topic[3])` (C1), tested — the 3 shape-colliding WASM contracts NOT flipped
- [x] Orphan predicate uses `coalesce(deployed_at_ledger, 0) = 0` (C2) — all 5,607 covered
- [x] Un-deployed-SAC orphans labeled `is_sac=true`, `contract_type=Token` — LIVE forward-fix (`derive_sac_overrides_from_events`, wired in `process.rs`) + batch (`sac-orphan-relabel`). Prod RUN → 0303.
- [ ] `/v1/contracts` no longer polluted by skeleton placeholders → **0307** (de-bundled)
- [ ] 0221 event-leak re-validated → **0307** (only if 0307 picks the side-table option)
- [x] Forward-fix prevents new WASM orphans → N/A: the split found 0 WASM orphans (Step 5 dropped)

## Implementation Notes

- **Live forward-fix** (`crates/xdr-parser/src/sac.rs`): `derive_sac_overrides_from_events`
  + the shared per-event core `sac_override_from_event_topics` (crypto-match
  gated). Wired at `crates/indexer/src/handler/process.rs` → `sac_overrides` →
  `prepare_with_sac_overrides` (`is_sac=true, contract_type=Token,
  wasm_uploaded_at_ledger=0`). +8 unit tests.
- **Batch relabel** (`crates/backfill-runner/src/sac_orphan_relabel.rs`, CLI
  `sac-orphan-relabel`): reuses the SAME gate over historical orphan events,
  re-INSERTs corrected RMT rows. dry-run, idempotent, PG short-circuit, e2e
  gated on `CLICKHOUSE_URL`. +3 tests.
- **Docs**: `docs/architecture/xdr-parsing/xdr-parsing-overview.md` updated per
  ADR 0032. No `crates/api/**` / `Cargo.toml` change → api-types codegen N/A.
- Verification: 254 xdr-parser + 3 backfill tests green; clippy `-D warnings`
  clean (xdr-parser + indexer + backfill-runner). Evidence:
  `notes/S-devil-advocate-full-crypto-split.md` + `notes/S-review-synthesis.md`.

## Design Decisions

### Emerged

1. **Scope split — code here, RUN to 0303.** Code-only task (forward-fix + batch
   tool); the prod relabel RUN / rebuild / reclassify / TRUNCATE live in 0303
   (scope updated there). Mirrors 0283 → 0303.
2. **`topics.last()`, not `topic[3]`.** The asset is the LAST topic across all 5
   SAC signatures and both protocol eras; `burn` is 3-topic (asset at index 2).
   The code is intentionally more correct than the "topic[3]" wording above.
3. **Shared gate `sac_override_from_event_topics`** — one crypto-match used by
   both the live path and the batch tool (parity IS the safety argument).
4. **RMT `version=0` override** — reuses the shipped 0220 sentinel (wins over the
   skeleton, loses to a real deploy), not a new mechanism.

## Future Work

- **0307** — SAC-skeleton `/v1/contracts` de-pollution (Step 3 research).
- **0303** — prod RUN, incl. the relabel's prod **OOM-query fix**
  (`fetch_orphan_events` must anchor on the ~5,607 orphan ids before running).
- Nits (review, optional): `version=1` sentinel hardening (uniform with 0220);
  fold the 3-way `topic_symbol_value` duplication into a shared helper.
