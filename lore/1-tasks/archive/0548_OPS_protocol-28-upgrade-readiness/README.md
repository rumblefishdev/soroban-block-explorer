---
id: '0548'
title: 'Protocol 28 (Adapter) readiness: Galexie 28.0.1 pin + stellar-xdr 27→28 before the 2026-09-16 pubnet vote'
type: OPS
status: completed
related_adr: []
related_tasks: ['0367', '0368']
tags: [indexer, xdr, infra, galexie, protocol-28, priority-high]
links:
  - 'https://stellar.org/blog/developers/adapter-protocol-28-upgrade-guide'
  - 'https://stellar.org/blog/developers/introducing-adapter-protocol-28-on-stellar'
  - 'https://hub.docker.com/r/stellar/stellar-galexie/tags'
history:
  - date: 2026-09-08
    status: active
    who: karolkow
    note: >
      Created 8 days before the mainnet vote, from the SDF upgrade
      announcement. First PLANNED execution of the protocol-upgrade bump —
      0367/0368 were both reactive (16 h silent ingestion stall, then a
      7.5k-message DLQ). 0367's own future-work list called for exactly this.
  - date: '2026-10-01'
    status: completed
    who: karolkow
    note: >
      Post-vote check 2026-09-30: 241,367 protocol-28 ledgers, no gap, 0 parse
      errors, ledger DLQ empty, lag alarm quiet. The prices repo's develop
      bumped to stellar-xdr 28 on 2026-09-14.
---

# Protocol 28 (Adapter) readiness

## Summary

Pubnet votes to protocol 28 on **2026-09-16 17:00 UTC**. Two independent things
break at that instant if untouched: the digest-pinned Galexie image still ships a
protocol-27 captive core (stops writing to S3 → ingestion starves), and the
workspace still pins `stellar-xdr = "27"`, which cannot decode the new XDR union
arms (parse error → DLQ). Both failures already happened once, on the 26→27
upgrade — this task is the same work done _before_ the vote instead of after.

## Acceptance Criteria

- [x] `galexieImageTag` pinned to the Galexie 28.0.1 ECR digest, read back from
      ECR (not copied from Docker Hub) — mirrored 2026-09-14 by pulling the Hub
      digest `--platform linux/amd64`; ECR `describe-images` and
      `batch-get-image` both report `sha256:1d511631…dcf01b` for tag `28.0.1`,
      identical to the Hub digest this time (single-architecture manifest, not
      rewritten on push). The 27.0.0 image `sha256:91eae7af…` stays in ECR as
      the pre-vote rollback target
- [x] ~~GitHub env `GALEXIE_IMAGE_DIGEST` updated~~ — not needed: nothing reads
      it since task 0390 (`docs/deployment.md`, Galexie recipe)
- [x] Galexie 28.0.1 live in prod, S3 exports flowing, before 2026-09-16 17:00 UTC
      — rolled out 2026-09-14 11:16 UTC, tip caught up at 12:12 UTC, ledgers
      contiguous across the restart gap (see the deploy section)
- [x] Workspace `stellar-xdr` = 28; `cargo check --workspace --all-targets` green
      (2026-09-10)
- [x] `ContractExecutable::ExternalRef` handled at all render sites, with a test
      per site; no fallback that mimics `wasm`. **Four sites, not three** — the
      fourth is `ScVal::ExecutableTag` in `scval.rs`
- [x] A testnet proto-28 ledger decodes clean — ledger 4,601,991, committed as a
      fixture and asserted to report `protocol_version = 28`. Weaker than it
      reads: the fixture also decodes under stellar-xdr 26 (review 2026-09-14);
      the constructed-arm round-trips are what fail under 27
- [x] Post-vote (checked 2026-09-30): 241,367 p28 ledgers from 64,458,446, no gap, 0 parse
      errors in 66.9M txs; ledger DLQ 0, its alarm OK since 08-27, lag alarm OK since 09-14
- [x] **Decided 2026-09-10** — what `wasm_hash` and the upgradeable chip say for
      an external-ref contract (gaps 1 and 2 above): both closed by option C,
      see "The model that closed them" and "Gap 2 is closed"
- [x] Production schema for this branch in place — both `soroban_contracts`
      columns with `DEFAULT NULL`, `contract_executable_refs` created
      (2026-09-11; see the incident above)
- [x] Sibling `prices` repo bumped in step with this one — its `develop` pins
      `=28.0.0` since 2026-09-14 (#314); its `master` stays on 27 until its next release
- [x] **Docs updated** — the expected `N/A` turned out to be wrong. Two
      architecture docs stated that a protocol upgrade is handled by bumping the
      `stellar-xdr` pin, full stop. Our own two incidents disprove that, so both
      now name the Galexie half and the compiler-invisible half:
      `technical-design-general-overview.md`,
      `infrastructure/infrastructure-overview.md`. Schema, endpoints, pipeline
      steps and topology are unchanged — those stay `N/A`
- [x] **API types regenerated** — the pin bump alone left `openapi.json`
      byte-identical; the executable-reference model then added
      `executable_owner` / `executable_tag` to `ContractDetailResponse`,
      regenerated with it

Context, plan, the two pre-vote incidents and every check are in
[notes/R-task-record.md](notes/R-task-record.md).
