# S — Implementation + read-only validation (2026-06-23)

Status: **developing**. Code committed on `fix/0294_sac-labeling-and-orphan-composition`
(PR #272 → develop).

## What was built

Recovered the core from the abandoned PR #264 (`git stash` snapshot), **selectively**
— only the 0294 files, NOT the full stash (which also reverted merged 0297/0293/
0291/0292/0307; rejected).

- **Detection-stage SAC gate (live ingest, the FINAL design):** `detect_nft_events`
  (`crates/xdr-parser/src/nft.rs`) runs the shared `sac_override_from_event_topics`
  gate up front; a crypto-proven classic-asset SAC event (asset `CODE:ISSUER` in the
  last topic, `derive_sac(asset)==emitter`) is SKIPPED before its i128 amount can be
  minted as a false NFT token*id. Stateless, per-event, cross-ledger, no quarantine
  needed for this class. (Earlier this session a persist-stage `derive_sac_overrides*
  from_events` override was tried, then REMOVED — the gate one stage too late; the
  detection-stage placement supersedes it. That function no longer exists.) See
  [[xdr-parsing-overview]].
- **Batch history-repair:** `crates/backfill-runner/src/sac_orphan_relabel.rs`
  (subcommand `sac-orphan-relabel --dry-run`). `--network` arg dropped (mainnet-only
  stack; wrong passphrase → gate rejects all → safe no-op).

## Read-only validation (chq, dev_read cert)

Reproduced `--dry-run` OFFLINE: exported orphan→event topics via chq (chunked), ran
the SAME shared gate locally. Result:

| metric                                                               | value     |
| -------------------------------------------------------------------- | --------- |
| orphans by predicate (`is_sac=false`, deploy 0/null, wasm_hash null) | **5,607** |
| emit a SAC-control event (in batch scope)                            | **5,558** |
| **crypto_confirmed** (`derive_sac(asset) == emitter`)                | **5,558** |
| **rejected by gate**                                                 | **0**     |
| emit no SAC-control event (left untouched)                           | 49        |

**Reads:** zero false positives — every SAC-event-emitting orphan is a real
un-deployed SAC, and ALL 5,558 confirm (the original "5,607/5,607" estimate was
essentially right). Only 49 emit no SAC-control event and are left untouched (tiny
residual). NB: a first pass under-counted (4,824) due to a truncated chunked
export — re-run with smaller chunks gave the correct 5,558.

## Fixes found during validation

1. **Batch query OOM** — `fetch_orphan_events` used `GROUP BY any(topics_xdr)` over a
   join on `soroban_events` (~344M rows) → 3.74 GiB, exceeds the read role's limit.
   Rewritten to two memory-safe passes: light `(id, strkey)` fetch, then chunked
   (`FETCH_CHUNK=500`) `LIMIT 1 BY` event fetch. **Validated** (chunks pass, full
   scan OOMs).
2. **Scope doc corrected** in the module header to the final 5,558 confirmed / 49
   left-untouched figures (an earlier 4,824/780 pass under-counted due to the
   truncated chunked export — superseded).

## Open / spawned follow-ups

- **Registry depollution (un-deployed SAC = asset, not a contract)** — the original
  Step-3 "/v1/contracts pollution" fix. Mainnet-verified (Soroban RPC) the orphans
  have no on-ledger instance. NOT a side-table (that was a hotfix); the fundamental
  fix is to stop writing their `soroban_contracts` rows + LEFT-join the references.
  Spawned to **0323** (full design there).
- **Classifier residual** (the ~49 non-emitters + bespoke-NFT `Other` gap) → **0317**.
- **Real flip on prod** — write-capable cert (read-only `chq` can't INSERT),
  operational → **0315** (OPS) / **0303** rollout.
- **WASM forward-fix + phantom** (AC5) → **0295**.
- **0221 event-leak re-validation** — superseded: the detection gate prevents the SAC
  leak at source, so no side-table-carried pre-window verdict is needed.

See [[G-orphan-split-queries]].
