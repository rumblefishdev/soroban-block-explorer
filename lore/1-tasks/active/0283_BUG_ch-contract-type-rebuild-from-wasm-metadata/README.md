---
id: '0283'
title: 'BUG: CH never writes Nft/Fungible verdicts to soroban_contracts — contract-type rebuild from wasm_interface_metadata + prod NFT reclassification'
type: BUG
status: active
related_adr: ['0046']
related_tasks: ['0118', '0217', '0220', '0221', '0228', '0231', '0259', '0282']
blocked_by: []
tags:
  [
    clickhouse,
    nft,
    quarantine,
    contract-classification,
    pre-launch,
    priority-high,
    effort-medium,
    layer-data,
  ]
milestone: 1
links:
  - docs/runbooks/0217_nfts_pending_migration_and_drain.md
  - docs/runbooks/0221_ch_drain_sac_from_nfts_pending.md
  - docs/runbooks/artifacts/phase6_validation_20260521.md
history:
  - date: 2026-06-10
    status: backlog
    who: karolkow
    note: >
      Spawned from a deep-dive triggered by 0231 NFT enrichment testing:
      prod CH hot nfts/nft_ownership are empty (0 rows) while pending
      holds 59.7M/138.5M and grows ~1M/day. Root cause found in code:
      CH stage writes contract_type verdicts only when WASM upload and
      contract deploy land in the SAME ledger (near-never on Soroban —
      separate txs), the "re-emission on next observation" path
      documented in ADR 0046 does not exist in code, and nothing
      rebuilds verdicts post-hoc — so `backfill-runner nft-reclassify`
      promotes 0 forever (empirically: promoted_nfts=0 in 0228 Phase 5).
      Full evidence chain in notes/S-deep-dive-root-cause.md.
  - date: 2026-06-10
    status: active
    who: karolkow
    note: 'Activated; work starts in worktree 0283-nft-reclassify.'
  - date: 2026-06-11
    status: active
    who: karolkow
    note: >
      Investigation session (Claude). Restored the small tables from the
      local CH backup snapshot_b_post_0252 into a throwaway container and
      ran Step 0 queries as a prod proxy (2026-05-21 state). Empirical
      confirmation + sizing, a SAC/asset-model deep dive, the crate-location
      decision (NOT backfill-runner → new `ch-maintenance-runner`), and the live-gap
      latency simulation (option c) all captured in
      notes/S-snapshot-findings-location-and-live-decisions.md. New Step 6
      added for the Bachini/i128 event-extraction gap.
  - date: 2026-06-11
    status: active
    who: karolkow
    note: >
      Scope consolidated into one task (operator decision): assets-fungible
      backfill + live fix pulled in (Steps 2, 5). Measured the batch pipeline
      end-to-end (~9 s full-scale) — the "heavy mutation / 0281 window" worry
      was over-cautious. Fixed `queries_ch.rs::contract_type_name` (2→nft,
      3→fungible) + test (DONE). LIVE design changed from inline dict/cache to
      a 3rd async reclassification Lambda (enrichment-worker pattern, scheduled/
      coalesced, singleton guard) — removes the 4 s-budget/dict concern. Live
      analysis + "why lookups are normally cheap" in the findings note.
  - date: 2026-06-11
    status: active
    who: karolkow
    note: >
      (pm) Indexer-reads claim verified: the PG path DID do cross-ledger DB
      reads at persist (reclassify_contracts_from_wasm + assets bridge +
      promote) — dropped at the 0241 CH cutover; bug #4 is a parity gap, not
      a design constant. Live decision RE-OPENED: dev-cost comparison favors
      the inline port (~2-3 d, no infra) over the 3rd Lambda (~4-6+ d);
      recommendation A, operator to confirm. Crate renamed to
      ch-maintenance-runner (consistent *-runner family). Classifier stays in
      xdr-parser (used by indexer at staging.rs:561; "not used in indexer"
      claim was false). Details in the findings note addendum.
  - date: 2026-06-11
    status: active
    who: karolkow
    note: >
      (eve) LIVE DECISION FINAL: inline in the indexer; 3rd-Lambda proposal
      dropped after CTO review of the research brief (brief delivered, then
      removed — content folded into Step 5 + the findings note). Fundament audit:
      quarantine is NOT speculative classification (API never reads pending;
      hot = WASM-confirmed only) — with inline G1+G9 it degrades to a DLQ;
      elimination ladder defined (inline → deploy-linkage fix → TRUNCATE/drop).
      Simulations on the full-scale snapshot killed the cost fears: batched
      lookup 4–8 ms flat, "peak 59 deploys" = 1 unique wasm hash, routing
      cache ~9 misses/day, dictionary option built+validated+rejected.
      CORRECTED: ~99.4% of pending stays after reclassify (no-deploy-link
      contracts) — NEW follow-up findings: deploy-linkage gap (4,461
      contracts) and SAC-skeleton exposure in /v1/contracts (294,963 rows).
      Step 5 rewritten as decided; AC updated. Earlier 2026-06-11 entries
      mis-attributed to stkrolikiewicz (stale session file) — corrected, the
      whole session was karolkow.
  - date: 2026-06-12
    status: active
    who: karolkow
    note: >
      Snapshot env re-verified after machine restart (ch-snap/ch-ui
      restarted, all 10 restored tables intact, Q1-Q4 reproduce 1:1).
      Step 0 split into 0a (snapshot proxy - DONE, results recorded in
      README as standing reference) and 0b (prod re-run - OPEN, mTLS);
      AC updated accordingly. Pausing here for now.
  - date: 2026-06-15
    status: active
    who: karolkow
    note: >
      Implementation session (Claude). LIVE inline fix landed for G1
      (deploy verdict via writer prefetch of prior-ledger WASM), G2
      (assets type-3 row on a Fungible verdict), G9 (event routing via
      cross-ledger verdict). G9 done as Option (b) per-ledger batched
      lookup, then upgraded to Option (c) lazy cache = the PG
      `ClassificationCache` pattern, consolidated into `domain`
      (single home; indexer/db-clickhouse re-export; the duplicate PG
      copy moved to .trash). G3 found UNNEEDED live (G1 + protocol
      ordering upload-before-deploy → nothing accumulates; closed). G5
      (name-clobber) CONFIRMED a real bug via devil's-advocate (PG used a
      column UPDATE, CH whole-row RMT clobbers) — DEFERRED (patch G5a/b or
      the fundamental name-side-table per ADR 0048). BATCH:
      `contract-type-rebuild` built in backfill-runner (staging+EXCHANGE,
      Rust classifier reuse, dry-run, idempotent) with the
      assets-fungible-backfill bundled as its Phase 5; `nft-reclassify`
      audited (complete, correct, semantically identical to the live
      router). Staging helpers deduped into `crate::ch_staging`
      (repair-tier1 / asset-aggregates / contract-type-rebuild).
      All edits local, NOT committed. Build + clippy green; new unit
      tests for G1/G2/G9 + cache + rebuild pass. Two follow-ups sharpened:
      deploy-linkage (the 4,461 orphans — `created`-only deploy filter
      drops `restored` instances, but restored is a CANDIDATE not the
      verified dominant cause; needs raw-XDR re-parse or Soroban RPC to
      confirm) and WASM-upgrade-never-reclassified (confirmed; the parser
      drops `updated` instances, pinned by test `skip_updated_contract_
      instance`; severity low/theoretical). New follow-up: cache lives in
      `domain` (move the legacy PG path off the trashed copy fully when PG
      is removed). Crate relocation to `ch-maintenance-runner` still
      deferred (logic developed in backfill-runner for now).
  - date: 2026-06-15
    status: active
    who: karolkow
    note: >
      (session 2, Claude) Adjacent-bug analysis + RPC verification +
      pattern synthesis (ADR 0049). SAC-skeleton suppression filter on
      /v1/contracts list was prototyped (PG queries.rs + CH queries_ch.rs)
      then REVERTED by operator (karolkow) — decision to fix the skeleton
      at the root (move the routing verdict out of the public registry)
      rather than band-aid it at read. WASM-upgrade fix REJECTED-as-naive:
      flipping the
      `created`-only filter to include `updated` fabricates wrong
      deployer/deployed_at_ledger and CLOBBERS the real deploy row under
      RMT(version=wasm_uploaded_at_ledger) — same family as G5; needs a
      writer-merge, folded into the G5 follow-up. Sub-agent audit of the
      indexer found 2 NEW issues: (a) `extract_account_states` drops
      `removed` for accounts → AccountMerge leaves a stale native-balance
      row (no zero tombstone; trustlines DO zero out) — medium; (b)
      `extract_contract_interfaces` dropping `Restored` (contract.rs:23)
      also defeats the G1 prior-verdict prefetch for TTL-restored WASM —
      medium-high coupling. SENIOR REFRAME of G5: `soroban_contracts.name`
      is the lone late+partial writer on the identity row; ADR 0048 already
      established "separate table per independent writer" but its blind spot
      is intra-writer two-cadence clobber. contract_type/wasm_hash do NOT
      need splitting (always written as complete rows). G5 options table:
      G5a fold same-ledger / G5b writer-merge / G5c name→side-table / G5d
      drop-name / (G5e version-only = rejected). RPC VERIFICATION of the
      4,461 deploy-linkage orphans (script .tmp-rpc-0283/verify.js, batched
      getLedgerEntries on mainnet.sorobanrpc.com): 200/200 sampled orphans
      = `instance_absent_or_archived` (NOT in live state), positive control
      Bachini found → method sound. CONCLUSION: RPC-live-backfill is a DEAD
      branch for this population. NEW hypothesis: orphan profile (is_sac=
      false + ALL deploy fields NULL) == G5 name-clobber victim profile, so
      part of the 4,461 may be G5 fallout, not a parser change-type gap —
      two discriminator CH queries handed off (orphan name-NOT-NULL count;
      orphan max event ledger vs head). All session edits local, NOT
      committed; verify.js is throwaway/uncommitted.
  - date: 2026-06-16
    status: active
    who: karolkow
    note: >
      (session 3, Claude) A SEPARATE NFT-event-SHAPE bug was found, FIXED, and
      ground-truth-validated — distinct from the wasm-link gap. A fresh-eyes
      5-agent deep-dive ("why orphans / can we enrich all real NFTs") surfaced
      that `detect_nft_events` (nft.rs) handled only Shape A (from/to in
      topics, token_id in data = SEP-50/OpenZeppelin) and SILENTLY dropped two
      real mainnet shapes: Shape B packed-data-vec (Bachini / ERC-721 ports,
      `topics=[Symbol], data=Vec(addr…, token_id)`) and Shape A2 token_id-as-
      extra-topic. Affected NFTs never reach pending at all → a silent,
      uncountable data-loss class. This CORRECTS open-problem #7: the i128
      refutation was right but tested the wrong axis — the drop is event SHAPE,
      not token_id type. FIX (local, uncommitted): unified `extract_args`
      (A/A2/B) + a `tracing::warn!` tripwire on symbol-matched-but-unparsed;
      map-data deferred (tripwire will surface real cases). VALIDATED on the
      real on-chain Bachini Mint event — decoded from raw XDR (stellar.expert
      /events + Soroban RPC: instance LIVE, wasm c5e2d06e, wasm contains
      owner_of+token_uri ⇒ classifier=Nft) — via `detect_real_mainnet_bachini_
      mint_event`. 244 xdr-parser tests green (+5), clippy clean. KEY: Bachini
      is NOT an orphan (it HAS a wasm); its 0-rows was the shape bug, proving
      shape-gap ⟂ wasm-link-gap. Wasm-link autopsy refined: orphans are stub-
      only rows; `created`-only deploy filter REFUTED as dominant cause under
      genesis-complete; candidates = parse_error deploy txs / phantom
      diagnostic refs (needs 1 orphan create-ledger meta to confirm; data-
      blocked). Link IS recoverable from CreateContract op-args (executable +
      preimage), today parsed-then-dropped at stage.rs:1491 — proposed forward-
      fix: persist `(contract_id, wasm_hash)` at ingest so new deploys can
      never become orphans (existing still need a raw-S3 re-parse).
      [Same-session follow-up: Shape A2 (token_id-as-extra-topic) was CUT —
      SEP-50/SEP-41 grounding + a getEvents sweep (255 events: 98% Shape A,
      0 B/A2, map=non-NFT) showed A2 has no spec and no on-chain instance.
      Final shapes = A (SEP-50/41 standard) + B (verified Bachini), map
      deferred; 243 xdr-parser tests green. See open-problem #7.]
---

# BUG: CH never writes Nft/Fungible verdicts — contract-type rebuild + prod NFT reclassification

## Summary

On the ClickHouse path, `soroban_contracts.contract_type` never receives a
WASM-derived `Nft`/`Fungible` verdict (except a same-ledger coincidence that
practically never happens), so the entire NFT promotion machinery
(`backfill-runner nft-reclassify`, runbook 0217 §Part 2) is a no-op: hot
`nfts`/`nft_ownership` stay at 0 rows, NFT endpoints E15/E16/E17 serve
nothing, and 59.7M/138.5M quarantine rows can never drain. The classification
INPUT already exists in CH (`wasm_interface_metadata`, 3,216 WASMs with
function lists) — what's missing is one rebuild step joining it back into
`soroban_contracts.contract_type`. This task adds that step and runs the
full reclassification on prod.

**Scope broadened (operator decision 2026-06-11): everything in one task.** The
same same-batch bug also under-populates `assets.asset_type=3` (Soroban
fungibles) — so the assets backfill + live fix are pulled in (Steps 2, 5).
**Live fix DECIDED (2026-06-11, after CTO review): inline in the indexer** —
re-implementation of the cross-ledger bridges the PG path had, in the CH
writer (~2–3 days, no new infra, fail-open); the 3rd-Lambda alternative was
evaluated and dropped (option analysis: Step 5 + the findings note).
New crate `ch-maintenance-runner` hosts the batch logic (one-shot history
rebuild + by-design-batch ops) — complementary to inline, not an alternative.

## Root cause (evidence)

1. **Verdict written only on same-ledger coincidence.**
   `crates/db-clickhouse/src/persist/stage.rs:376-389` — deploy row gets
   `Nft`/`Fungible` only if the WASM was classified "in the same ledger"
   (comment verbatim). `wasm_classification` map is built per stage call
   from this call's `contract_interfaces` (`stage.rs:343-363`), and a stage
   call covers exactly one ledger (`persist.rs:63-99`). On Soroban, upload
   WASM and create-contract are separate transactions (1 op/tx) — almost
   always different ledgers. Non-SAC deploys therefore persist the parser
   default `Other` (`crates/xdr-parser/src/state.rs` deployment tests).

2. **ADR 0046's "re-emission on next observation" does not exist.**
   `route_for` (`stage.rs:909-918`) consults only `verdict_by_contract`
   built from THIS ledger's `contract_rows`; events emit at most stub rows
   with `contract_type: None`, which the map skips. A later NFT event from
   a classified contract still routes to Pending. The ADR documents intent,
   not code.

3. **Nothing rebuilds verdicts post-hoc.**
   `crates/backfill-runner/src/repair_tier1.rs:327` passes `sc.contract_type`
   through unchanged. `crates/backfill-runner/src/nft_reclassify.rs:193-194`
   promotes `WHERE contract_type = 2` — a set nothing ever populates.
   Empirical proof: 0228 Phase 5 run on the full merged backfill reported
   `promoted_nfts=0` (`docs/runbooks/artifacts/phase6_validation_20260521.md`).

4. **Live ingest has the same gap.** Indexer (post-0241 cutover) persists
   via the same `persist_ledger_clickhouse` path
   (`crates/indexer/src/handler/mod.rs:30`). New NFT contracts deployed
   today also stay `Other`. Pending grows ~1M rows/day.

Full deep-dive (incl. corrections to prior assumptions, prod state
numbers, verification SQL): [notes/S-deep-dive-root-cause.md](notes/S-deep-dive-root-cause.md).

## Empirical findings (local snapshot proxy, 2026-06-11)

Step 0 queries run on the restored `snapshot_b_post_0252` (2026-05-21 / Phase 6
state — re-run on live prod for go-live sizing). Full detail incl. asset/SAC
model, contracts-vs-rows, pending breakdown, location + live decisions:
[notes/S-snapshot-findings-location-and-live-decisions.md](notes/S-snapshot-findings-location-and-live-decisions.md).

- **Verdict breakdown**: of 26,401 non-SAC contracts, exactly **1** ever got
  `Nft` (type=2), 2 got `Fungible` (type=3) — confirms root cause empirically.
- **Would-be-Nft after rebuild: 107 contracts** (vs 1 today); would-be-Fungible
  **3,937** with the exact classifier predicate (an earlier 4,159 figure used a
  looser OR-predicate). Decision is per **contract**; promote acts per **row**.
- **Promote volume**: 107 NFT collections hold **11,023** token rows in
  `nfts_pending` + **19,451** ownership events (one collection `CBHUX3RS…` =
  10,056 tokens). Real data exists — `promoted_nfts` will be >0, not 0.
- **What reclassify does (CORRECTED 2026-06-11 pm)**: promote ~0.02% (Nft),
  drop SAC+Fungible (~0.5%), and **~99.4% STAYS in pending** — it belongs to
  4,461 contracts with **no deploy/wasm_hash link at all** (deploy never
  observed; top offender `CDP5RUMSC7YJ…` alone = 4.86M rows). Unresolvable at
  write time AND at rebuild time → the TRUNCATE decision (Step 6) governs it,
  and the **deploy-linkage gap is a new follow-up finding** (earlier claim
  "~99.97% dropped" was based on a LEFT JOIN mislabel — FixedString non-match
  fills zero-bytes, not NULL).
- **Bachini** (`CDA5FGE4…`, the one verified real NFT): sits as `Other`
  (rebuild fixes it) **but has 0 rows in either pending table** → a separate
  event-extraction gap, see **Step 6**.
- **Assets — SAME bug class, second table.** `asset_type` enum is
  explorer-synthetic (0 Native / 1 ClassicCredit / 2 Sac / 3 Soroban-fungible).
  `asset_type=3` (Soroban bespoke fungible) has only **2 rows** — the _same_
  same-batch-coincidence bug: ~3,935 would-be-fungible non-SAC contracts are
  **missing from `assets`**. The PG persist path has a late-WASM assets bridge
  (`insert_assets_from_reclassified_contracts`); the **CH path never ported
  it**. Now IN SCOPE (Steps 2 + 5) per operator decision.

## Why this gates launch

- E15/E16/E17 (`/nfts*`) serve zero data on prod (0259 documented the gap;
  this task is its actual unblocking dependency).
- 0231 (NFT `token_uri` enrichment) Step 4 prod drain has an empty queue
  until hot `nfts` fills; 0282 needs a real NFT population sample.
- ~27.6M accumulated SAC-leak rows (0221) in pending get dropped by the
  same reclassify run — separate manual drains become unnecessary.

## Open problems (unsolved — standing list as of 2026-06-15 session 2)

The core 0283 classification gap is fixed (live G1/G2/G9 + batch rebuild,
all local/uncommitted). These remain OPEN. **Status below reflects a 7-agent
adversarial verification pass (2026-06-15 session 2)** — each problem
independently confirmed/refuted by a deep-dive sub-agent.

> **SPAWN-OUT (2026-06-16, karolkow):** the side problems were spun off to
> dedicated backlog tasks on develop, leaving 0283 to its core deliverable
> (the rebuild + prod reclassification run = #9):
> **#1 + #6 → [0294](../../backlog/0294_BUG_sac-labeling-and-orphan-composition/README.md)** (SAC labeling + orphan composition, DB-gated);
> **#3 + #5 → [0295](../../backlog/0295_BUG_parser-change-type-extraction-gaps.md)** (parser change-type gaps);
> **#7 + CAP-67 → [0296](../../backlog/0296_BUG_nft-event-extraction-completeness/README.md)** (NFT/event extraction — code parked as patches, reverted out of this branch);
> **#2 structural-close + #2b → [0297](../../backlog/0297_FEATURE_contract-name-enrichment-and-bytes-decode.md)** (contract-name enrichment + bytes-decode).
> **#8 (ch-maintenance-runner relocation): DROPPED — won't do.** > **#9 (operational/prod): STAYS here** — it is 0283's own finish line, not a
> separate problem. **ADR 0049 deleted; its Family-A/Method-2 framing inlined
> into the spawned tasks** (in-body "ADR 0049" mentions below are historical).

1. **deploy-linkage — ~4,310 orphans.** _Verdict: real; cause = genuine
   missing-deploy (archival/parser), NOT G5._ **Discriminator query RAN
   (2026-06-15 snapshot): of 113,067 non-SAC contracts, `with_name = 0` — zero
   names anywhere, so `name_only` (the G5-clobber signature) = 0.** This REFUTES
   the earlier "deploy-linkage ⊆ G5" lean: orphans have no name → they are not
   clobber victims. They are genuinely missing deploy/wasm (5,607 have NULL
   contract\*type; 4,310 have no deployed*at_ledger), consistent with the RPC
   200/200-absent result. **Refined (operator: index is genesis-complete, no
   gaps):** with no window/gap, these contracts' CREATE ledger WAS indexed but
   their deploy was NOT extracted (they exist only as Pass-2 stub rows from
   events). Root = a **deploy-extraction / meta gap at create time**
   (meta-unavailable ledgers, or a deploy shape `extract_contract_deployments`
   doesn't match) — NOT a dropped `restored`, NOT a window gap, NOT G5.
   Fix = investigate why those creates were missed (needs their create-ledger
   meta), then re-parse / patch the extractor. Quarantine-only impact.
   \_Refined + DB-confirmed (2026-06-16): orphans are stub-only rows; dominant
   cause = **un-deployed SACs via CAP-67** (not mis-parsed deploys; `parse_error`
   candidate refuted; top offender crypto-proven a SAC). Real NFTs in the set
   are ~1.5%, mostly un-enrichable. **DECIDED: DEFER — no orphan code now.** Full
   evidence, buckets, and the decisive split-queries are spawned to **0294**.*
2. **G5 name-clobber.** _Mechanism CONFIRMED in code, but EMPIRICALLY INERT._
   The 2026-06-15 discriminator shows `with_name = 0` across 113,067 non-SAC
   contracts → the name-write path writes NOTHING to `soroban_contracts.name` on
   CH, so the clobber never fires (no name rows to outversion the deploy). The
   code hazard is real (clobbered NULL wasm_hash would defeat the rebuild join
   `contract_type_rebuild.rs:224`), but current impact = ZERO. **DEAD/LATENT —
   DEPRIORITIZED.** Root cause of name=0 PROVEN (2026-06-15): RPC dump of
   Bachini's ContractInstance shows EMPTY instance storage, no `Symbol("name")`
   anywhere — Soroban tokens expose the name via a `name()` FUNCTION (read by
   simulateTransaction), not a persisted ledger entry, so
   `extract_contract_data_name_writes` can never match. Consequence: contract
   NAMES are off-ledger → an ENRICHMENT job (RPC `name()`, ADR 0048/0231 family),
   NOT a parser/Family-A fix. **GUARDRAIL SHIPPED (2026-06-15, local/uncommitted):**
   the name-only row in stage.rs now uses `wasm_uploaded_at_ledger = 0` (was
   current ledger) so a real deploy always outversions it → the partial name row
   can NEVER clobber the deploy identity (name kept only when it is the sole row);
   plus a TRIPWIRE `tracing::warn!` that fires if `contract_name_writes` is ever
   non-empty (dormant path activating). Full merge-discipline NOT done (moot —
   names dead/empty + headed to enrichment); structural close = names→enrichment
   side-table when that work lands. **Agent deep-dive (2026-06-15)
   CONFIRMED root cause (a):** producer (`scval.rs:53-56` → `{"type":"sym",
"value":"name"}`) and consumer (`is_symbol_name_key`, state.rs:199) match
   byte-for-byte — the "key-shape mismatch" hypothesis (b) is DISPROVED, tests
   are faithful, the parser is correct. Names are off-ledger (function `name()`);
   nothing to fix in xdr-parser; populating `name` is an enrichment job (RPC
   `name()` simulate), not a parser change.
   - **Minor secondary bug (NEW, low):** `ScVal::Bytes` name decode mismatch —
     producer base64-encodes Bytes (`scval.rs:45`), but `decode_scval_string`
     hex-decodes (`state.rs:243`) → a real bytes-typed name would fail to decode;
     the unit test uses a hand-written hex string and masks it. Tiny/edge-case,
     separate follow-up.
3. **WASM-upgrade never reclassified.** _Agent verdict: CONFIRMED but **LOW
   severity** (high conf)._ Classification is function-NAME based, so most
   upgrades preserve the interface → invisible; the `created`-only filter is a
   legitimate guard against the deployer-clobber a naive fix would cause.
   Correct fix = parser handle `updated` + writer merge-discipline + cache
   invalidation. Fold into the G5 follow-up. NOT fixed.
4. **interface-`Restored` gap.** _RESOLVED as NOT-NEEDED (operator: indexing is
   genesis-complete, no gaps)._ A `restored` ContractCode implies a prior
   `created` that — under complete indexing — we already captured into
   `wasm_interface_metadata` (our store is not subject to on-chain archival), so
   `restored` is a pure duplicate and brings nothing. The `| Restored(e)` edit
   was prototyped then REVERTED. Consequence: with a complete index,
   `wasm_interface_metadata` coverage is COMPLETE for all deployed contracts →
   interface coverage is NOT an enrichment blocker.
5. **AccountMerge balance tombstone.** _Agent verdict: CONFIRMED but **LOW-MED**
   (high conf)._ Tempered: can't merge an account holding trustlines (native row
   only), task 0228 already ACCEPTS merged accounts as a "skeleton floor", and
   it self-heals on StrKey re-creation. But the API DOES serve the stale balance
   and a native-XLM aggregate could be inflated. Fix = emit a `balance=0` native
   row at the merge ledger (account_id from the change key). NOT fixed.
6. **SAC-skeleton root-derivation.** _Agent verdict: PARTIAL (med-UX/high)._
   294,963 phantom rows pollute the public registry, BUT API correctness is
   intact (only quarantine; hot nfts=0) and skeletons are distinguishable
   (`deployed_at_ledger IS NULL`). **Red-team caveat: the read-filter is
   STRICTLY safer for the 0221 guarantee than the root fix** — Method 2 must
   re-populate pre-window SAC verdicts in the side table or the 0221 leak
   returns. The verdict rows are load-bearing for G9 routing. DECIDED (karolkow):
   fix at the root via a side-table, NOT a read-filter. HARD CONSTRAINT the
   side-table MUST satisfy: carry the SAC `Token` verdict for **pre-window** SACs
   (the skeleton's current G9-routing job) or the 0221 leak returns — re-validate
   0221 as the acceptance gate. **SPAWNED → task 0294** (bundled with the orphan
   composition #1 — they converge on the same SAC-derivation gap; ADR 0049 was
   deleted 2026-06-16, its framing inlined into the spawned tasks).
7. **NFT event-SHAPE extraction gap.** _FIXED + chain-validated 2026-06-16 →
   spawned to **0296**._ Supersedes the earlier "Bachini/i128 REFUTED" — the drop
   was the event SHAPE (packed `data=Vec`), not the i128 type. Fix + tripwire
   validated on the real on-chain Bachini Mint; full taxonomy, SEP grounding,
   sweep, and the parked code live in 0296.
8. **G5 side-table = `ch-maintenance-runner` crate relocation** still deferred
   (rebuild logic lives in backfill-runner for now).
9. **Operational/prod (need live CH / mTLS, not code):** Step 0b prod queries,
   Step 3 prod run, instrumentation + verification, RTT probe, E15/16/17 smoke,
   docs (ADR 0046, runbooks 0217/0221, clickhouse-pilot).

## Implementation Plan

### Step 0a — verification queries on the snapshot proxy (DONE 2026-06-11, re-verified 2026-06-12)

The four queries from
[notes/S-deep-dive-root-cause.md §Verification queries](notes/S-deep-dive-root-cause.md)
run against the local `ch-snap` restore of `snapshot_b_post_0252_20260526`
(2026-05-21 / Phase 6 state — the closest available proxy for prod until the
mTLS run happens). Results — recorded here as the standing reference numbers:

| Query                                            | Result                                                                                  |
| ------------------------------------------------ | --------------------------------------------------------------------------------------- |
| Q1 verdict breakdown (`soroban_contracts FINAL`) | SAC(0) 294,963 / Other(1) 21,523 / **Nft(2) 1** / Fungible(3) 2 / NULL 4,875            |
| Q2 would-be-Nft contracts after rebuild          | **107** (would-be-Fungible: 3,937 exact-predicate)                                      |
| Q3 Bachini `CDA5FGE4…` sanity                    | `contract_type=1` (Other), `is_sac=false`, has wasm — bug confirmed on a known-real NFT |
| Q4 promote volume in pending                     | **11,023** `nfts_pending` tokens / **19,451** `nft_ownership_pending` events            |

Re-verified 2026-06-12 after a machine restart (containers restarted, all 10
restored tables intact, all four queries reproduce 1:1). Full detail in the
[findings note](notes/S-snapshot-findings-location-and-live-decisions.md).

### Step 0b — re-run on prod CH (OPEN, required before go-live)

Same four queries against the live Hetzner CH — the snapshot numbers above are
the 2026-05-21 state; live 2026-06-10 pending was already larger (59.7M /
138.5M) with the SAC-leak regrown (8.6M / 18.9M), so prod counts WILL differ
in the drop buckets (promote volume should stay in the same ballpark). This
re-sizes the run and confirms nothing drifted. Requires mTLS cert
(`infra-hetzner/ca/issue-client-cert.sh`) + `~/.config/soroban-prod.env`.

**PROD RESULTS (2026-06-16, via `chq`) — partial (quota cap hit):**

| metric                                       | prod                                                              | snapshot       |
| -------------------------------------------- | ----------------------------------------------------------------- | -------------- |
| Current verdicts (`soroban_contracts FINAL`) | **Nft 1 / Fungible 2** / SAC 311,153 / Other 107,457 / NULL 5,607 | Nft 1 / Fung 2 |
| **Would-be after rebuild** (wasm classify)   | **Nft 125 / Fungible 4,118**                                      | 107 / 3,937    |
| **Promotable NFT token rows**                | **11,214** across **40** collections                              | 11,023         |
| **NFT collections with ZERO pending rows**   | **85 of 125** (classify Nft but 0 surfaced tokens)                | —              |
| name non-empty                               | **0 of 424,220** (G5 off-ledger)                                  | —              |
| orphans (is_sac=0, no deploy, no wasm)       | **4,310**, ALL with pending = **51.5M rows**                      | 4,461          |

**Key reads:** (1) the rebuild flips ~125 contracts to Nft, surfacing **11,214 promotable token rows** = the enrichment population — confirms the snapshot on live prod. (2) **85 of 125 NFT-classified collections have 0 pending rows** — strong prod-wide signal that the NFT event-shape gap (→ task 0296) suppresses far more than the recent-window sweep suggested (some may also be genuinely inactive). (3) Bachini (`CDA5FGE4…`): Other + wasm + deploy `54599504` → rebuild flips it to Nft; NOT an orphan.

**Quota-blocked (reset hourly):** phantom-caller magnitude + full crypto-match orphan split (task 0294) — `dev_read` 2B rows/h exhausted by the event scans.

### Step 1 — `ch-maint contract-type-rebuild` (NEW crate, not backfill-runner)

**Location decision (2026-06-11):** this does **NOT** belong in
`backfill-runner` (its charter is S3 historical ingestion, a one-shot job
complete per task 0228 — the CH-maintenance ops squatting there only because
that crate already had a CH sink). Create a new crate
**`crates/ch-maintenance-runner`** (bin `ch-maint`), a CH post-hoc maintenance
toolbox modeled on the standalone-CLI precedent of `backfill-enrichment-runner`.
**Relocate** `repair-tier1`, `asset-aggregates`, `nft-reclassify` into it too
(same family; rebuild → nft-reclassify is one ordered pipeline). Rationale +
deps in [the findings note](notes/S-snapshot-findings-location-and-live-decisions.md).

New module `crates/ch-maintenance-runner/src/contract_type_rebuild.rs`, modeled
on `repair_tier1.rs` (staging table + `EXCHANGE TABLES` swap):

- Read `wasm_interface_metadata` (wasm_hash → metadata JSON with
  `functions[].name`), classify each hash in Rust by reusing
  `xdr_parser::classify_contract_from_wasm_spec` (exact PG parity — do
  NOT reimplement the rule set in SQL).
- Rebuild `soroban_contracts` into staging:
  `contract_type = classify(wasm) if wasm_hash matches AND NOT is_sac AND
verdict ∈ {Nft, Fungible}`, else passthrough. SAC rows untouched.
- `EXCHANGE TABLES`, drop staging. Idempotent, `--dry-run` support
  (counts per verdict transition), same logging shape as `nft_reclassify`.

> **`status` command?** backfill-runner's `Status` is S3-ingest progress —
> untouched by this work, stays as-is. Any status/report for the maintenance
> ops (verdict breakdown, would-be-Nft, promote/drop volumes) is a NEW
> `ch-maint` concern (`--dry-run` summaries or a `ch-maint status`).

### Step 2 — `ch-maint assets-fungible-backfill` (NEW — pulled into scope)

The same bug hits a second table: `assets.asset_type=3` (Soroban bespoke
fungible) is emitted only on the same-batch WASM+deploy coincidence
(`xdr-parser/src/state.rs:853-871`), so ~3,935 would-be-fungible contracts are
missing from `assets` (only 2 present). The rebuild (Step 1) makes
`soroban_contracts.contract_type=3` authoritative, so this is then mechanical:

- One-shot `INSERT INTO assets` of the missing type-3 rows:
  `SELECT asset_type=3, contract_id=sc.id, name=sc.name, … FROM soroban_contracts
sc WHERE sc.contract_type=3 AND NOT is_sac AND NOT EXISTS(matching asset row)`.
- Identity is the 4-tuple `(asset_type, asset_code, issuer_id, contract_id)`;
  type-3 rows carry `contract_id` only (code/issuer NULL). `--dry-run` + counts.
- Lives in `ch-maintenance-runner` next to `contract-type-rebuild` (shares the CH
  client + staging helpers). SAC (type-2) and classic (type-1) untouched.

### Step 3 — run on prod: rebuild → assets-backfill → nft-reclassify

0. **Rehearsal (before touching prod):** run the real implemented binaries
   against the full-scale local snapshot (`ch-snap` machine) — same data
   shape, zero risk; expected results are all measured (107/3,937 flips,
   11,023/19,451 promote, ~9 s).
1. `ch-maint contract-type-rebuild --dry-run` → compare with Step 0; real run.
2. `ch-maint assets-fungible-backfill --dry-run` → real run.
3. `ch-maint nft-reclassify --dry-run` → real run. Existing code
   (relocated into `ch-maintenance-runner`) covers BOTH `nfts_pending` AND
   `nft_ownership_pending` (promote type=2, drop SAC/fungible, OPTIMIZE FINAL).
4. Record before/after counts (pending totals, promoted, dropped, hot totals,
   assets type-3 count).
5. **Runtime measured (local snapshot, full-scale):** whole pipeline ~**9 s**
   — rebuild 0.43 s + EXCHANGE 0.13 s, promote 0.25 s, `ALTER DELETE` full
   drain 1.15 s (48.8M) + 6.64 s (112M), OPTIMIZE ~0.2 s. The "coordinate with
   0281 maintenance window / heavy ALTER DELETE" worry is over-cautious — it's
   seconds. Still verify on the single remote Hetzner node under live merge
   load; don't start a second run while a previous one's mutations are still
   merging (`system.mutations` check).

### Step 4 — API: `contract_type_name` fix (DONE) + verify contracts-list

`GET /v1/contracts` is a pure consumer of `soroban_contracts.contract_type`
(`queries_ch.rs:103`, reads via `FINAL`; `filter[type]=nft` → `= 2`), so the
rebuild flips its counts **1 Nft / 2 Fungible → 107 / ~3,937 with NO API code
change**. **DONE 2026-06-11:** fixed `queries_ch.rs::contract_type_name`
(2→nft, 3→fungible) + its stale test (was the CH/PG divergence that returned
`contract_type_name:null` for the new rows). Verify the live counts after the
prod run.

### Step 5 — LIVE fix: **DECIDED — inline in the indexer** (2026-06-11, after CTO review)

The 3rd-Lambda proposal was evaluated against measurements and dropped; full
option analysis + devil's advocate per option in the
[findings note](notes/S-snapshot-findings-location-and-live-decisions.md).

**What it is:** re-implementation, on the CH writer, of the three cross-ledger
bridges the PG path ran in production (dropped at the 0241 cutover):
`reclassify_contracts_from_wasm` (`indexer/handler/persist/write.rs:240-325`),
`insert_assets_from_reclassified_contracts` (`write.rs:543-584`),
`promote_pending_nfts_to_hot` (`write.rs:337-417`). PG itself is being removed
from the project — we port the **algorithm**, not the database. The stage
stays pure (no DB); the post-stage step runs in the writer/handler (both
already hold a `clickhouse::Client` — `writer.rs:72`, `handler/mod.rs:130`).

**Scope (gap inventory G1–G9, Addendum 2):**

- **G1** verdict at deploy: one batched `wasm_hash IN(...)` lookup, only on
  deploy-bearing ledgers (**0.18%** of ledgers).
- **G2** `assets` type-3 row on a Fungible verdict (same trigger).
- **G3** promote pending→hot on an actual Nft flip (~once per 4 days) — note:
  with G1 live + protocol ordering (upload precedes deploy) nothing accumulates
  to promote going-forward, so this needs no separate live step; historical
  pending is covered by the batch backstop (see Addendum 3).
- **G5** name-write clobber fix (name-only RMT row must not NULL out
  wasm*hash/deployer — read-merge before re-emit). \_DEFERRED (2026-06-15) —
  confirmed real bug (PG used a column UPDATE; CH whole-row RMT clobbers).
  Options: G5a suppress the same-ledger name-only row (cheap), G5b cross-ledger
  prefetch+merge, or the fundamental fix — move `name` to its own RMT side
  table joined at read (ADR 0048 pattern), which eliminates the two-writer
  clobber class entirely.*
- **G9** verdict at event-routing time: lazy in-memory verdict cache
  (5,707 distinct emitting contracts; **~9 cache-misses/day**; never cache
  unknown; Nft/Fungible verdicts are immutable once set). G9 **also closes the
  0221 write-time SAC leak** (SAC-emitted events get dropped at routing instead
  of leaking into pending — the leak regrew 8.6M/18.9M rows since Phase 5).
- G2 also covers the `assets.name` mirror (PG pass-2 behavior never ported).
- G6 (asset aggregates) + G8 (first_seen watermarks) stay batch **by design**
  (every-ledger triggers); G4 (SAC) already covered in stage; G7 obsolete.

**Measured (full-scale snapshot):** batched lookup **4–8 ms** flat
(IN 1…1000); the feared "59-deploy ledger" = 59 instances of ONE wasm hash →
1 query; per-contract promote (10k rows) **5–8 ms**; option-(d) dictionary
built and validated end-to-end (load 110 ms, 508 KiB, dictGet 3–7 ms) —
works but adds nothing at these frequencies, rejected. Net cost: **0 ms on
~99% of ledgers, 4–8 ms otherwise** vs the ~4 s budget. Only unmeasured
variable: Lambda→Hetzner RTT (assumed 30 ms — confirm with one probe).

**Fail-open ladder (zero correctness risk):** cache hit → route (0 ms); miss →
one batched SELECT (4–8 ms); DB doesn't know / query fails → behave exactly
as today (`Other` + quarantine) → batch backstop drains later. The new code
can only degrade to current behavior, never below it.

**Guardrails (devil's-advocate verified, Addendum 4):** the live flip path
does promote-INSERT only — pending cleanup stays **async/deferred to the
batch backstop** (never `mutations_sync=1` inline); lookup cost under part
fragmentation is 10–18 ms (not 4–8) — verify on prod in Step 0; recent
deploy-rate is ~0.4% of ledgers (2× the historical mean) — still ~0.008% of
budget. Lambda region confirmed **eu-central-1** (RTT assumption conservative).

**Endgame:** with G1+G9 live, pending degrades from a pipeline stage to a
**DLQ** (inflow = only unknown-deploy contracts = the deploy-linkage bug).
Elimination ladder: inline fix → fix deploy-linkage gap (follow-up) →
TRUNCATE / drop the pending tables entirely ("classify once, correctly").

### Step 6 — 0217 Part 2 TRUNCATE decision (explicitly OUT of first run)

Do NOT truncate pending in the same pass. After rebuild+reclassify, what
remains (~99.4%) belongs to the 4,461 no-deploy-link contracts (the
deploy-linkage bug) plus genuine `Other`. TRUNCATE only AFTER the elimination
ladder's step 2 (deploy-linkage fix) — truncating earlier silently destroys
data that may include real NFTs (the exact Patch-C mistake ADR 0046 rejected).

### Step 7 — Bachini / i128 token_id event-extraction gap

Step 0 found Bachini (`CDA5FGE4…`, the only verified real mainnet NFT) has
**0 rows in both pending tables** — so even a correct rebuild + reclassify
surfaces nothing for it. Its events were never extracted; deep-dive flags it
as SEP-39 with **i128 token_id**, a shape the event parser likely doesn't
capture. This is a **different subsystem** (XDR event extraction, not
classification) — keep as a tracked step here, but it may graduate to its own
task. Without it, "NFTs fixed" still leaves the flagship NFT empty.

### Step 8 — docs

- **ADR 0046 correction**: the CH "re-emission on next observation"
  promotion path is documented but not implemented; amend with the actual
  mechanism (rebuild + reclassify, live worker) and link this task.
- Update `docs/runbooks/0217_nfts_pending_migration_and_drain.md` §Part 2
  CH section: prepend the contract-type-rebuild step; commands `ch-maint …`.
- Mark 0221 manual drain runbook as subsumed by `nft-reclassify`.
- `docs/architecture/database-schema/clickhouse-pilot.md` §quarantine: add the
  rebuild step to the promotion lifecycle.

## Acceptance Criteria

- [x] **Step 0a** verification queries run on the snapshot proxy; results
      recorded (Q1 verdict breakdown 294,963/21,523/1/2/4,875; Q2
      would-be-Nft 107; Q3 Bachini=Other; Q4 promote 11,023/19,451).
      Re-verified 2026-06-12.
- [ ] **Step 0b** same queries re-run on prod CH (mTLS); results recorded —
      go-live sizing (drop buckets will differ: live pending 59.7M/138.5M + regrown SAC-leak).
- [ ] New crate `crates/ch-maintenance-runner` (bin `ch-maint`) created; `repair-tier1`,
      `asset-aggregates`, `nft-reclassify` relocated out of backfill-runner into it.
      (DEFERRED 2026-06-15 — logic developed in `backfill-runner` for now;
      shared staging helpers already deduped into `crate::ch_staging`.)
- [x] `contract-type-rebuild` implemented — DONE 2026-06-15 in `backfill-runner`
      (staging+EXCHANGE, Rust classifier reuse, `--dry-run` flip/asset counts,
      idempotent). Live-CH integration run still pending (logic unit-tested).
- [x] `assets-fungible-backfill` implemented — DONE 2026-06-15, **bundled as
      Phase 5 of `contract-type-rebuild`** (insert type-3 rows from
      `contract_type=3`, `NOT EXISTS` guard; PG-bridge parity).
- [x] Unit test (2026-06-15): deploy with a prior-ledger `Nft` WASM verdict
      flips to `Nft`; SAC untouched; no-verdict stays `Other`; Fungible→type-3
      asset; Nft→no asset (G1/G2 stage tests). Live-CH integration test of the
      rebuild itself still pending.
- [ ] Prod run executed: rebuild → assets-backfill → `nft-reclassify`;
      before/after counts recorded (hot `nfts`/`nft_ownership` non-zero —
      local proxy ~11,023 / 19,451 promote; SAC/fungible pending dropped;
      assets type-3 grows ~2 → ~3,937).
- [ ] E15/E16/E17 smoke against prod after the run (links 0259).
- [x] **`queries_ch.rs::contract_type_name` fixed** (2→nft, 3→fungible) + test
      updated — DONE 2026-06-11. Verify `GET /v1/contracts` counts post-run.
- [x] **LIVE inline fix — G1 + G2 + G9 IMPLEMENTED** (2026-06-15, fail-open,
      writer-prefetch → pure-stage maps): G1 deploy verdict, G2 assets type-3,
      G9 event routing via cross-ledger verdict + lazy `ClassificationCache`
      (consolidated in `domain`). **G3 closed** — unneeded live (G1 +
      upload-before-deploy ordering ⇒ nothing accumulates to promote).
      **G5 (name-clobber) DEFERRED** — confirmed real bug (PG column-UPDATE vs
      CH whole-row RMT clobber); patch G5a/b or fundamental name-side-table
      (ADR 0048). 3rd-Lambda alternative dropped after CTO review. Live-CH
      integration + prod verification still pending (next 3 AC items).
- [ ] RTT Lambda→Hetzner measured (one probe via mTLS) — confirms the last
      assumption (30 ms) behind the live numbers.
- [ ] **Inline step instrumented + verified on prod**: emit per-ledger timing
      of the new step, cache hit/miss counters, and a fail-open counter
      (lookup skipped due to error). After deploy, compare a week of prod
      metrics against the simulated numbers (~0 ms typical / 10–40 ms on
      deploy-WASM-miss ledgers / fail-open ≈ 0) — closes the "if it's real"
      question with production data.
- [ ] Follow-up task spawned: **deploy-linkage gap** — 4,461 contracts emit
      events but have no deploy/wasm_hash ever (99.4% of pending; top
      `CDP5RUMSC7YJ…` = 4.86M rows); blocks the TRUNCATE endgame.
      **Confirmed defect (0283):** `extract_contract_deployments`
      (`state.rs:59`) filters `change_type == "created"` only, so a
      state-archival **`restored`** contract-instance entry (which DOES carry a
      `wasm_hash`) is silently dropped — the parser emits `restored`
      (`ledger_entry_changes.rs:154`) and every sibling extractor accepts it
      (`state.rs:352`); the deploy extractor's `created`-only filter is the
      asymmetry. **But `restored` is only a CANDIDATE cause, NOT verified as
      the dominant one** (devil's-advocate audit): the 4,461 "never seen at
      all" bucket more plausibly comes from meta-unavailable / unparsed deploys;
      `restored` likely explains a tail. **Not verifiable on the CH snapshot**
      (the dropped entries aren't there; soroban_events giant absent locally) —
      verify by re-parsing raw S3 XDR for a sample orphan (e.g. `CDP5RUMSC7YJ…`)
      or RPC `getLedgerEntries`. Fix splits into: (a) widen the deploy filter to
      `restored`/`updated` (cheap; also fixes the WASM-upgrade follow-up below),
      and (b) RPC-backfill wasm_hash for orphans that have no `created` at all.
      **Ordered plan (do NOT re-parse everything first):** 1. **Verify the hypothesis cheaply via Soroban RPC** — `getLedgerEntries`
      on a sample of orphan contract instances (`LedgerKeyContractInstance`):
      do they exist on-chain, with a `wasm_hash`, and is the verdict what we
      expect? Confirms whether the orphans are recoverable + which mechanism
      (restored vs never-created) dominates. 2. **Only then decide the fix mechanism:** RPC-backfill (one-shot fetch of
      wasm_hash for orphans, no re-parse) **vs** parser re-parse of raw S3
      XDR (widen `created`→`restored`/`updated`, heavier, full re-ingest).
      Pick based on step-1 findings (volume, recoverability, cost).
- [ ] Follow-up task spawned: **SAC skeleton exposure** — 294,963 derived
      skeleton rows (92% of `soroban_contracts`) visible in `/v1/contracts`
      with no filter (real violation of "no speculative user-facing rows").
- [ ] Follow-up task spawned: **WASM upgrades never re-classified** — the
      parser drops contract-instance `updated` entries (`state.rs:59`,
      created-only), so an upgraded contract keeps its deploy-time
      wasm_hash/verdict forever on BOTH paths (pre-existing, PG parity;
      found by devil's-advocate audit, Addendum 4).
- [ ] Bachini/i128 SEP-39 event-extraction gap (Step 7) — investigated;
      tracked here or graduated to its own task.
- [ ] ADR 0046 amended (re-emission correction → actual mechanism: inline
      bridges + batch backstop) + runbook 0217/0221 updated; command strings
      `backfill-runner …` → `ch-maint …`.
- [ ] **Docs updated** — `clickhouse-pilot.md` §quarantine (rebuild step +
      pending-as-DLQ); ingestion-pipeline docs for the new inline writer step;
      infra topology N/A (no new Lambda).
- [ ] **API types regenerated** — `crates/api/**` touched (contract_type_name) + new `Cargo.lock` → run `nx run @rumblefish/api-types:generate` before
      commit (label change likely no-op on the spec, but the gate checks it).

## Notes

- TRUNCATE of pending is deliberately deferred (Step 6) — destructive; safe
  only after the deploy-linkage fix (elimination ladder step 2).
- **Quarantine is NOT speculative classification** — it's the opposite: the
  API never reads `*_pending`; hot tables receive only WASM-confirmed rows
  (pre-quarantine design measured 99.4% garbage in `/v1/nfts*`). With the
  inline fix it degrades to a DLQ; rows arriving there = bug signal.
- Expected scale (measured on the snapshot proxy): 294,963/321,364 contracts
  are SAC (~92%); would-be-Nft is **107 contracts → ~11,023 token rows**.
  Empty-ish hot tables after a CORRECT run are a product reality, not a bug —
  re-confirm on live prod (Step 0).
- Investigation env still running locally: container `ch-snap` (restored
  small tables from `~/snapshots/snapshot_b_post_0252`, port 8123) + `ch-ui`
  (port 3488). Originals/backup untouched; benches ran on copies.
