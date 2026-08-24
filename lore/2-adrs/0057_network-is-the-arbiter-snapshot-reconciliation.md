---
id: '0057'
title: 'The network is the arbiter — snapshot reconciliation as a standing invariant'
status: accepted
deciders: [karolkow]
related_tasks: ['0463', '0502', '0503', '0316', '0492', '0321']
related_adrs: ['0055', '0056']
tags: [clickhouse, data-integrity, backfill, snapshot, policy]
links: []
history:
  - date: '2026-08-19'
    status: accepted
    who: karolkow
    note: >
      Extracted from the 0463 session that root-caused the balances
      same-version ties (the 2026-06-23 merge-tombstone fix re-parsed over
      pre-fix rows) and built the checkpoint-snapshot toolchain. These are the
      durable POLICY decisions; the reusable tool's shape stays in task 0502,
      the recurring audit in 0503, the operational runbook in
      docs/backfills.md.
  - date: '2026-08-20'
    status: accepted
    who: karolkow
    note: >
      Decision 4 amended: the manifest-versus-consensus check (folding the
      bucket list and comparing it against the ledger header's
      bucket_list_hash) was built, live-verified once, and REMOVED as
      over-engineering. It defended against a forged or stale manifest from
      the network's own reference publisher, while the realistic failure
      modes — bad download, truncation, substituted file — are covered by the
      per-bucket SHA-256 that stays. Resurrect from git history if a
      third-party mirror is ever added as a source.
---

# ADR 0057: the network is the arbiter

## Context

Our tables are insert-only ReplacingMergeTree: an "update" is a newer-versioned
row, and the merge keeps the highest version. Three defect classes follow:

1. **Between-runs ties.** A re-parse of old windows with semantically changed
   writer code emits rows at the SAME versions the old code used. Different
   content at an equal version is resolved arbitrarily by the merge — and by
   `argMax` reads. Proven at scale: 1,238,583 native keys after the 2026-06-23
   merge-tombstone fix, each randomly showing 0 or a stale balance.
2. **In-ledger ordering.** Two transactions touching one entity in one ledger
   must collapse to the LAST state in chain application order.
3. **Coverage gaps.** A change stream cannot see what never changed: 60% of
   live trustlines had no row at all.

An insert-time tiebreaker ("later run wins") was considered for class 1 and
set aside: it requires rebuilding every RMT table (the engine's version
parameter is immutable), and it resolves to _whichever code ran last_ rather
than to the truth. The intent behind it — a backfill's new data MUST win —
stands, and is delivered by this ADR's mechanism instead.

## Decision

**The Stellar network's own state, read from the history-archive checkpoint
snapshot, is the arbiter of what our tables should say. Reconciliation
against it is a standing invariant, not a one-off.**

Concretely:

1. **Versioning.** Every state row versions on the ledger of the FACT it
   records — the entry's own `lastModifiedLedgerSeq` for live data. Absence
   facts (closures, ghosts) have no ledger of their own and version at the
   snapshot's checkpoint, meaning "true at or before". Never a window
   boundary, never a synthetic stamp (the task 0492 defect).
2. **After any historical re-parse** the reconciliation is MANDATORY: the
   same-version tie query (0503), then `snapshot-seed` — its dry-run IS the
   four-way comparison, reviewed before `--execute`. Dead-entity divergence
   is repaired outright at the
   checkpoint version, which deterministically supersedes both sides of any
   tie. Operational detail lives in `docs/backfills.md`.
3. **Live-entity same-ledger divergence is quarantined, not auto-healed** —
   it means one of two parser versions misread that ledger, and adopting
   either side silently would bury the evidence. If the bucket proves noisy,
   the agreed extension is a heal-from-NETWORK mode (adopt the chain's value
   at the checkpoint version, full list to an artifact) — the "new data wins"
   outcome, sourced from something better than either run.
4. **Every snapshot consumer verifies bucket content**: each downloaded
   bucket's SHA-256 (over the decompressed XDR) against the manifest's hash,
   so a truncated or substituted file fails loudly. The manifest itself is
   taken on trust (TLS + the SDF archive). A further check of the manifest
   against the consensus-signed `bucket_list_hash` in the ledger header was
   built, live-verified once, and REMOVED in the 2026-08-20 review as
   over-engineering: it defended against a forged-or-stale manifest from the
   network's own reference publisher, while the realistic failure modes (bad
   download, wrong decode) are covered by the per-bucket hashes and the RPC
   spot-check. Resurrect from git history if a third-party mirror is ever
   added as a source.
5. **A new state table joins the reconciliation or records why not.** Any
   future table derived from ledger entries MUST either be added to the
   snapshot comparison (0503's per-entity table names every entry type's
   owner) or carry an explicit exemption in its schema comment. "We never got
   to it" produced the 60% gap; it does not get to produce the next one.
6. **In-ledger ordering is a writer obligation**: fold per key, last state in
   application order wins, with a regression test per writer (all eight state
   writers covered as of 2026-08-19). Fact tables carry an intra-ledger order
   column in their sort key, aggregate per transaction before insert, or are
   presence tables whose collapse is the semantic.

## Consequences

- Full backfills are two-step by policy: re-parse, then reconcile. The seed
  run after a backfill is not optional hygiene; it is what makes the
  backfill's intent actually take effect under equal-version inserts.
- The snapshot toolchain becomes standing infrastructure (task 0502 extracts
  the decoder; 0503 runs the audit on a schedule). Its bucket inputs are
  content-verified against the manifest, so a corrupted download is a loud
  failure, never a silent misread.
- Rows are never deleted; wrongness is superseded. The pre-correction export
  is therefore the only pre-image and must be archived before every
  `--execute`.
- Post-seed, the comparison's residue becomes a pure indexing-correctness
  monitor: any in-window discrepancy is a parser defect with a reproduction
  ledger attached.
