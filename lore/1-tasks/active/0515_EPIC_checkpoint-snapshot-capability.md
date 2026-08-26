---
id: '0515'
title: 'EPIC: the checkpoint-snapshot capability — one entry point for everything seeded from network state'
type: EPIC
status: active
related_adr: ['0055', '0056', '0057']
related_tasks: ['0463', '0492', '0497', '0499', '0502', '0503', '0504', '0514']
tags: [epic, index, snapshot, clickhouse, data-integrity, priority-medium]
links: []
history:
  - date: '2026-08-24'
    status: active
    who: karolkow
    note: >
      Opened as an index over work that grew inside 0463 and then spread across
      seven tasks with no shared entry point. Holds no detail of its own — the
      register, the trust ledger, and the durable rules, each pointing at the
      task that owns it.
---

# EPIC: the checkpoint-snapshot capability

## Why this exists

Reading pubnet's full state from the history archive was built **inside** task
0463, as the means to an end (issue #377). It turned out to be a capability
rather than a step: it is the only source that can answer _what does the
network have that we do not_, and seven tasks now depend on it or extend it.

None of them shares an entry point. Reconstructing the picture meant holding
seven task numbers in your head. **This file is that entry point.** It is an
INDEX — every fact lives in exactly one owning task, and this file only gists
and links. If a detail appears here and nowhere else, it is in the wrong place.

## What the capability is

The SDF history archive publishes the complete state of pubnet at every
checkpoint as a bucket list — 21 gzipped files, ~4.4 GB, newest-first, keyed by
content hash. Decoding it gives DISTINCT live entries at that ledger.

That matters because our index is a stream of CHANGES since ledger floor
50,457,424, and most of chain history predates it. An entry that never changed
since then has no row on our side at all, so **no query over our own data can
see it — not even to count it.** A re-parse cannot help: it replays ledgers we
already have, and the missing entries are not in them.

Measured consequence: **19.3M classic trustlines the network holds and we never
had a row for**, all of them last modified below our floor.

## Register

| task     | what it owns                                                                          | state                                                                |
| -------- | ------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| **0463** | built the decoder and the seed; account-detail zero balances + signers                | active — writer deployed, seed dry-run verified, `--execute` pending |
| **0502** | extract the decoder into its own crate; the seed stays a consumer                     | backlog                                                              |
| **0503** | exhaustive completeness audit, every entity both directions; orphan holders           | backlog                                                              |
| **0504** | five entry types parsed and discarded (offers, claimable balances, data, TTL, config) | backlog                                                              |
| **0499** | merge `lp_positions` into `balances` (ADR 0056) — pool shares ride the same snapshot  | backlog                                                              |
| **0514** | native balance written low on Soroban transactions; owns the root cause AND the heal  | backlog                                                              |
| **0492** | provenance for seeded rows — no synthetic watermarks                                  | backlog                                                              |
| **0497** | retire `repair-tier1` — MIN semantics off RMT state tables                            | backlog                                                              |

Dependency worth stating: **0502 does not block 0463.** The seed ships from
`backfill-runner` as it stands; extraction is a refactor for the next consumer,
not a prerequisite for the first one.

## Trust ledger — what is verified, and by what

The point of recording this is that "we checked" is not a claim until the
method is stated. Every chain check below uses an **independent** XDR/StrKey
implementation, not our Rust, so a shared misreading cannot pass. Method:
[0463 notes/V-chain-audit-method.md](0463_FEATURE_account-detail-zero-trustlines-and-signers/notes/V-chain-audit-method.md).

| population                             | evidence                                                                                            |
| -------------------------------------- | --------------------------------------------------------------------------------------------------- |
| rows the seed INSERTS as live holdings | 2,000 chain probes over two runs — present, identity echoed, frozen, amount exact                   |
| every new `assets` row                 | all 97k structurally audited — id↔identity bijection, CRC on every issuer                           |
| signers from the seed                  | 5,000 accounts, thresholds + signer set + ORDER identical                                           |
| signers from the LIVE writer           | 377 / 377 identical                                                                                 |
| closures                               | 712 absent from chain; 5 merges and 6 trustline removals matched to the exact ledger's transactions |
| the write path itself                  | exercised end to end on a local ClickHouse — ~65.5M rows, four tables row-exact                     |
| classic ghosts                         | **NOT verifiable** — no source carries their StrKey (see 0503)                                      |

Honest limit, worth re-reading before trusting any of the above: sampling
bounds a **systematic** defect, not a random one. A wrong derivation hits every
row of its class and a fixed sample finds it; fifty randomly corrupted rows in
45M it will not.

## Durable rules

Each of these was learned the expensive way. They belong to the capability, not
to whichever task hit them first.

1. **Deployment order.** The lifecycle writer deploys FIRST, then the seed runs
   against a checkpoint taken after it. Reversed, every removal in the gap was
   written by the old writer as a plain zero at a ledger ABOVE the checkpoint;
   ReplacingMergeTree keeps the higher version, the closure loses, and the
   ghost is resurrected — as a zero-balance row that the new read filter
   deliberately shows. Satisfied permanently as of 2026-08-24: the writer is
   deployed, so every future run is on the right side.
2. **Version on the entry's own ledger, never on a window boundary.** Live
   facts version on `lastModifiedLedgerSeq` so the live writer wins whatever
   the load order; absence facts (closure, ghost) version on the checkpoint,
   meaning "true at or before". A synthetic stamp is the 0492 defect.
3. **The floor is the discriminator.** A missing entry BELOW our ledger floor
   is a coverage gap; one AT or ABOVE it means we index wrong. Post-seed this
   becomes a pure correctness monitor.
4. **A short read is indistinguishable from a real one.** Every input carries a
   floor: fewer known ids means more ids judged absent, means more dimension
   stubs — a truncated read manufactures rows for entities that already exist.
   The snapshot needs a floor for the same reason, in the opposite direction: a
   short snapshot would close tens of millions of live holdings.
5. **Read production in slices.** Both the balances read and the dimension-id
   read are sliced on key. `max_execution_time` counts the time spent SENDING
   rows, and the ceiling is load-dependent — it passes until it does not.
6. **Never verify our decoder with our decoder.** Chain checks re-implement
   StrKey and the XDR from the spec.

## Sequenced 2026-08-26 (owner): B → D → E

With the seed executed and audited, the question stopped being "does this work"
and became "what does this capability become". Options were laid out and three
were taken, in this order. The order is the decision — each one is a
precondition for the value of the next.

**B — extract the decoder into its own crate ([0502](0502…)).** First, because
everything else builds in the wrong place until it happens: the decoder and the
verdict rule currently live in `backfill-runner`, which is a tool for LOADING.
The seed should be one consumer of the capability, not its owner. Nothing
blocks this; it is a refactor with an existing plan.

**D — model the five discarded entry types ([0504](0504…), audit in
[0503](0503…)).** 78.7M records per pass are decoded and thrown away. The
concrete payoff is not completeness for its own sake: **claimable balances are
exactly the ~567k XLM residual the supply reconciliation currently INFERS
rather than measures.** The decoder already reads those records. Doing D before
any recurring report is deliberate — a periodic report that still infers one
term would harden an estimate into a fact, which is precisely the mistake
caught on 2026-08-26 with the fee pool.

**E — settle `audit-harness`.** Discovered while answering this question: that
crate's README lists `2c — DB vs raw archive XDR re-parse (ground truth)` as a
FUTURE phase. **The snapshot comparison is that phase, delivered** — and
delivered better, over the whole population rather than a sample. Meanwhile the
crate itself is Postgres-era (`sqlx`, `DATABASE_URL=postgres://…`) and cannot
run against a CH-only stack, and one of its three binaries (`horizon-diff`)
targets a source this project has ruled out as legacy. Its Phase 1 — pure SQL
invariants, internal consistency, no network — is genuinely complementary to
the snapshot (that catches divergence FROM the network; this catches
self-inconsistency WITHIN our own tables) and is worth porting to ClickHouse.
Either port it or archive it behind a banner; leaving a dead crate that reads
as live is the trap.

### Deliberately NOT taken

**A recurring/scheduled reconciliation.** Tempting — the dry-run is already a
full health report, and a weekly series would turn `divergent SAME ledger` and
the two defect signals into trends. Rejected for now on the grounds that the
moment this becomes an automated job is the moment nobody reads its output. It
stays a pass a human runs deliberately and reads. Revisit after D, when the
report has no inferred terms left.

**A dashboard, or wiring it into CI.** Same reason, more so.

## What this EPIC is for next

An audit of the whole capability, planned from one place rather than seven.
The inputs are already here: the register says what exists, the trust ledger
says what is proven and how, the rules say what must not be re-derived. The
open question an audit has to answer is coverage — which entities have been
compared against network state in BOTH directions, and which have only ever
been checked against themselves.

## Acceptance criteria

This EPIC closes when the capability is no longer spread:

- [ ] 0463 shipped and verified in production
- [ ] every entity in 0503's table compared in both directions, or explicitly
      deferred with a reason and an owner
- [ ] the decoder extracted (0502) with at least one consumer besides the seed
- [ ] no task in the register carries a measurement that exists only in this
      file
