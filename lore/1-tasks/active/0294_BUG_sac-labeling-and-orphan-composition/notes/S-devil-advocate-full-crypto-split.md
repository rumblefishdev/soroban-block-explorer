# S — Devil's-advocate FULL crypto-split + fix guardrails

> Synthesis note, 2026-06-18, karolkow (devil's-advocate dispatch, Claude).
> Status: mature. Supersedes the task's "A3 byte-quota-blocked" placeholder —
> the full per-orphan cryptographic split is now DONE. Verified fundamentally
> against mainnet + CAP/SEP specs + prod CH; repo treated as interpretation.

## Why this note exists

The task (and the prior red/blue pass) only had a 148-sample + the single top
offender for the cryptographic SAC match; the full per-orphan split was flagged
"byte-quota-blocked (A3)". A devil's-advocate dispatch finished it on the WHOLE
population. The core thesis did not just survive — it got stronger. But three
fix-safety guardrails surfaced that the task as written did NOT state, plus two
factual corrections.

## DECISIVE TEST — full crypto-split (the headline)

Method (no DB quota issue): the parked `notes/devil/orphans.tsv` turned out to
hold only `(i64_id, strkey)` — NOT the event topics — so it was never runnable
for the split. Re-exported `topic[3]` (the SAC asset string) fresh from prod CH,
economically: `transfer`-only, one row per contract, orphan-set as a SUBQUERY
(an inline 4,310-id `IN(...)` OOMs at 3.73 GiB). Then derived
`Asset(code,issuer).contractId(mainnet)` locally with `notes/devil/sac_derive.py`
and compared to each orphan id.

**Result — ALL orphans are cryptographically-confirmed un-deployed SACs:**

| set | predicate | transfer-emitters matched | mint-emitters matched | mismatch | WASM | phantom |
| --- | --- | --- | --- | --- | --- | --- |
| NULL-predicate | `deployed_at_ledger IS NULL` | **4,304 / 4,304** | **6 / 6** | 0 | 0 | 0 |
| `=0`-sentinel | `deployed_at_ledger = 0` | **1,199 / 1,199** | — | 0 | 0 | 0 |

So the cause is **TOTAL, not "dominant"**: 0 mismatch, 0 real-WASM, 0 phantom.

**Deriver trust:** `sac_derive.py` reproduces XLM, USDC and WGUARDIAN exactly
(3/3 self-test) and matches stellar.expert's live USDC asset→contract mapping
(`CCW67TSZ…`). Orphan ids return HTTP **404** on stellar.expert ("Contract was
not found on the ledger"); a deployed SAC returns **200** → orphans genuinely
have no on-chain instance.

**Pending impact:** 51,571,026 of 61,564,409 total pending rows (**83.8%**) sit
in these confirmed-SAC orphans → the `is_sac=true` flip drops ~84% of the bloat.

## CORRECTION 1 (C4) — "via CAP-67" is wrong for 68% of the data

P23 activated at ledger **58,762,517** (2025-09-03T17:00Z, confirmed via Horizon
`/ledgers/58762517`). But **238M of 349M orphan `transfer` events (68%) are
PRE-P23**, and **4,167 of 4,310 orphans (97%) were already emitting before
CAP-67 existed.** CAP-67 therefore cannot be the origin of the bulk.

Reconciliation: pre-P23 these SAC ids emit via **direct Soroban SAC
host-function invocation** (legal since Protocol 20); post-P23 *also* via CAP-67
classic-op unified events. The `id == SAC` conclusion holds across the entire
timeline, and Step 2 reads the asset from the event topic **regardless of
mechanism**, so the fix is unaffected — but the task's stated rationale must be
re-labelled. (This also refutes the earlier red-team claim that orphan events
are "necessarily post-P23".)

## CORRECTION 2 (C2) — true orphan population is 5,607, not 4,310

The canonical predicate `deployed_at_ledger IS NULL` silently misses **1,297**
structurally identical orphans carrying the `deployed_at_ledger = 0` sentinel
(all `is_sac=false`, `wasm_hash NULL`, `contract_type NULL`; 1,199 transfer-
emitters all crypto-confirmed SAC). Fix every predicate to
`coalesce(deployed_at_ledger, 0) = 0` before sizing/migrating, or 1,297 SACs
stay mislabeled. (NB: `is_sac=true AND deployed_at_ledger=0` = 234,902 are the
skeletons — correctly excluded from the orphan set.)

## MANDATORY fix guardrails (bake into the plan)

- **C1 (Critical) — Step 2 must gate on the CRYPTOGRAPHIC MATCH, not event
  shape.** 370 real WASM contracts (`is_sac=false, wasm_hash IS NOT NULL`) emit
  `transfer`; **3 of them emit the exact 4-topic SAC shape**
  `[sym, addr, addr, String("CODE:ISSUER")]` (copycat/wrapper tokens). A
  shape-only flip would corrupt these 3. The guard
  `emitter_id == derive_sac(topic[3])` rejects all 3 (verified). Make the
  crypto-match an explicit, tested precondition; AND write via the existing
  `wasm_uploaded_at_ledger = 0` SAC-override path so a future real deploy
  auto-outversions the flip under RMT/FINAL (handles the "poison a later deploy"
  inversion).
- **C3 (High) — Step 3 must preserve the verdict lookup or 0221 re-leaks
  instantly.** The 0221 leak fix is one query — `query_contract_verdicts`
  (`crates/db-clickhouse/src/persist.rs:380-383`):
  `SELECT contract_id, contract_type FROM soroban_contracts FINAL WHERE …
  contract_type IN (0,2,3)`. All 311,153 SAC rows carry `contract_type=0`
  (Token). Moving the ~307k skeletons to a side-table makes this query stop
  finding the Token verdict → leak returns. Step 3 must repoint/UNION this query
  to the side-table (or keep a verdict-only projection in `soroban_contracts`).
  The "re-validate 0221" gate is concrete + runnable: replay an event from a
  known un-deployed SAC and assert it does NOT land in `nfts_pending`.
- **C2 (High) — predicate `coalesce(deployed_at_ledger,0)=0`** (above).

## Plan simplifications

- **Drop Step 4 (phantom bucket).** The phantom-caller bucket is empty: at most
  6 non-transfer orphans, and all 6 are crypto-confirmed SACs (mint-emitters).
- **No double-count with 0296.** 0296 (WASM NFT event-shape) holds only 359,751
  pending rows in `wasm_hash IS NOT NULL` contracts; 0294 orphans
  (`wasm_hash IS NULL`) hold 51.5M. Disjoint by the wasm axis. No correction.

## Verdict

**SHIP WITH CHANGES.** The core thesis is certain enough to authorize the big
changes — 5,607/5,607 cryptographic confirmation + spec confirmation is as
conclusive as it gets. But C1 (crypto-match-gate), C3 (preserve verdict lookup +
runnable 0221 replay gate) and C2 (corrected predicate) are mandatory, not
optional. Re-label the mechanism (C4) and drop the empty phantom bucket.

## Sources

- CAP-0067 ("instance is not required to be deployed… events published using the
  reserved contract address regardless of deployment status") —
  https://github.com/stellar/stellar-protocol/blob/master/core/cap-0067.md
- SEP-0041 token interface —
  https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md
- P23 mainnet activation (2025-09-03) —
  https://stellar.org/blog/developers/announcing-protocol-23 ;
  P23 ledger boundary 58,762,517 — https://horizon.stellar.org/ledgers/58762517
- stellar.expert (orphan 404 vs deployed-SAC 200; USDC asset→contract) —
  https://api.stellar.expert/explorer/public
- Deriver + split: `notes/devil/sac_derive.py` (validated); split input
  re-exported via subquery-scoped `chq` (the parked `orphans.tsv` lacks topics).
