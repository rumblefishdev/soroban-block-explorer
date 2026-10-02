---
id: '0605'
title: 'Protocol 29: Galexie 28.0.1 stalled at the pubnet vote — bump to 29.0.0 (no XDR change)'
type: OPS
status: active
related_adr: []
related_tasks: ['0367', '0548']
tags: [ingestion, galexie, protocol-29, incident, priority-high]
links:
  - 'https://hub.docker.com/r/stellar/stellar-galexie/tags'
  - 'https://github.com/stellar/stellar-galexie/pull/96'
  - 'https://github.com/stellar/stellar-core/compare/v28.0.1...v29.0.0-internal'
  - 'https://github.com/stellar/stellar-horizon/pull/234'
history:
  - date: 2026-10-01
    status: active
    who: karolkow
    note: >
      Created during the outage. Pubnet voted to protocol 29 at 17:00 UTC;
      Galexie 28.0.1 stopped exporting at ledger 64,717,644. Third occurrence
      of the 0367 failure mode, and the first upgrade with no advance work.
---

# Protocol 29: Galexie bump

## Summary

Pubnet moved to protocol 29 on 2026-10-01 17:00 UTC. The Galexie image was
still 28.0.1 (captive core 28), which cannot apply protocol-29 ledgers, so it
stopped writing to S3 and ingestion stalled. Protocol 29 changes no XDR, so the
whole migration is the Galexie image: 29.0.0, captive core
`29.0.0-3589.4eb833373`.

## Stan teraz

- Done: cause found, protocol 29 content checked, decode of P29 ledgers checked.
- Done: Galexie 29.0.0 mirrored into ECR and pinned in `production.json`.
- Next: `deploy-production-ingestion`, then the contiguity check.
- In force: no Rust change — `stellar-xdr` stays at 28.0.0.

## Context

**The outage.** Last Galexie object `FC247CB3--64717644.xdr.zst`, written
17:00:04 UTC; the network was then at 64,717,978 on protocol 29. From then on
the live task logged, every 5 s,
`History: Skipping catchup: incompatible core version or invalid local state` —
the exact line from 0367. `production-galexie-ingestion-lag` went to ALARM at
17:08:56 UTC. The indexer was healthy throughout: ingest queue and DLQ at 0
messages, because nothing reached the bucket.

**What protocol 29 contains** (stellar-core v28.0.1 → v29.0.0-internal).
No CAP. A fix-and-security release:

- DEX offer crossing is more accurate; offers that would clear for 0 are
  filtered in the overlay.
- Liquidity-pool hops no longer count toward the offer-crossing limit, so some
  path payments that failed under 28 now succeed.
- Wasm cost inputs now count custom sections and `br_table` targets, so rent
  for newly uploaded code rises.
- Max message size 5 MB, max tx set 4 MB; overlay and handshake hardening.

None of these needs code here: we store the results, fees and amounts the meta
reports, and never recompute them.

**No XDR change.** stellar-core v29 pins the same `stellar-xdr` commit as v28
(`9c9c145953`); Horizon's protocol-29 PR states "No XDR change and no SDK bump".
`LedgerCloseMeta` stays v2. Checked by decode as a positive control: with
`stellar-xdr` 28.0.0, mainnet 64,717,644 (last P28) and 64,717,645–647 (first
P29) and testnet 4,970,000–001 all decode, `ledger_version` 28/29 as expected.
Six ledgers — the proof is the identical XDR pin, the decode only confirms it.

**`stellar-xdr` 28.0.1** (2026-09-29) only enforces `VecM` length on serde
deserialisation. We decode binary XDR and never deserialise XDR types from
JSON, so it is not part of this task.

## Implementation

1. Mirror `stellar/stellar-galexie@sha256:5269dfd9…` (tag 29.0.0, a manifest
   list with one linux/amd64 entry) into ECR `production-galexie` as `29.0.0`.
2. Pin the digest ECR reports in `infra/envs/production.json → galexieImageTag`
   (`docs/deployment.md`, Galexie recipe).
3. `make -C infra deploy-production-ingestion`. The 28.0.1 image
   `sha256:1d511631…` stays in ECR as the rollback target.

## Acceptance Criteria

- [x] `galexieImageTag` = the ECR digest of Galexie 29.0.0, read back from ECR —
      `sha256:5269dfd9…a495a3`, identical to the Hub digest: ECR stored the
      manifest list as pushed (one linux/amd64 entry, `sha256:538ad0fb…`, also
      present in ECR)
- [ ] Galexie 29.0.0 live, S3 exports resumed from 64,717,645
- [ ] `ledgers` contiguous across the gap (count = distinct = range), DLQ 0,
      ingestion-lag alarm back to OK
- [x] Protocol 29 checked for XDR changes — none; no Rust change needed
- [x] **Docs updated** — N/A: the Galexie recipe in `docs/deployment.md` and
      the upgrade note in `infrastructure-overview.md` already describe this
      exact procedure; schema, endpoints and pipeline unchanged
- [x] **API types regenerated** — N/A: `crates/api`, `Cargo.*` untouched

## Future Work

- Queued on `stellar-xdr` `main`, not in any protocol yet: contract-spec type
  names widen from 60 to 1024 characters (ungated), CAP-88 millisecond close
  time (new `StellarValue` arms) and CAP-87 ML-DSA cost types. The first can
  break our Wasm interface-spec parsing once SDKs emit long names.
- The 0367 healthcheck item is still open: `pgrep -x stellar-core` stayed
  healthy for this whole stall; only the lag alarm noticed.
