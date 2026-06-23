# S — Implementation + read-only validation (2026-06-23)

Status: **developing**. Code on branch `fix/0294_sac-labeling-and-orphan-composition`
(local, uncommitted).

## What was built

Recovered the core from the abandoned PR #264 (`git stash` snapshot), **selectively**
— only the 0294 files, NOT the full stash (which also reverted merged 0297/0293/
0291/0292/0307; rejected).

- **Forward-fix (live ingest):** `xdr_parser::derive_sac_overrides_from_events` +
  `sac_override_from_event_topics` (`crates/xdr-parser/src/sac.rs`), wired in
  `crates/indexer/src/handler/process.rs` (merged into `sac_overrides` alongside
  the existing asset path). Closes the `state.rs` gap where payment/transfer-only
  SACs (no trustline change) persisted as `is_sac=false` orphans. 8 new unit tests
  incl. the crypto-match gate (rejects a bespoke emitter). See [[xdr-parsing-overview]].
- **Batch history-repair:** `crates/backfill-runner/src/sac_orphan_relabel.rs`
  (subcommand `sac-orphan-relabel --dry-run`). `--network` arg dropped (mainnet-only
  stack; wrong passphrase → gate rejects all → safe no-op).

## Read-only validation (chq, dev_read cert)

Reproduced `--dry-run` OFFLINE: exported orphan→event topics via chq (chunked), ran
the SAME shared gate locally. Result:

| metric                                                               | value     |
| -------------------------------------------------------------------- | --------- |
| orphans by predicate (`is_sac=false`, deploy 0/null, wasm_hash null) | **5,607** |
| emit a SAC-control event (in batch scope)                            | **4,827** |
| **crypto_confirmed** (`derive_sac(asset) == emitter`)                | **4,824** |
| **rejected by gate**                                                 | **0**     |
| unparseable export rows                                              | 3         |

**Reads:** zero false positives — every SAC-event-emitting orphan is a real
un-deployed SAC. The earlier "5,607/5,607" claim was an overclaim: ~780 orphans emit
no SAC-control event and are intentionally left untouched (separate bucket).

## Fixes found during validation

1. **Batch query OOM** — `fetch_orphan_events` used `GROUP BY any(topics_xdr)` over a
   join on `soroban_events` (~344M rows) → 3.74 GiB, exceeds the read role's limit.
   Rewritten to two memory-safe passes: light `(id, strkey)` fetch, then chunked
   (`FETCH_CHUNK=500`) `LIMIT 1 BY` event fetch. **Validated** (chunks pass, full
   scan OOMs).
2. **Scope doc corrected** in the module header (4,824 confirmed / ~780 out of scope).

## Open

- **Characterize the ~780 non-emitters** — quota-blocked (`dev_read` 1 TiB/h, reset
  09:00). Likely non-SAC stubs / phantom. Needs its own classification.
- **Real flip on prod** — write-capable cert (read-only `chq` can't INSERT) →
  operational, belongs to **0303** rollout.
- **Side-table** (registry depollution, original step 3) — separate concern, dropped
  from this task.

See [[G-orphan-split-queries]].
