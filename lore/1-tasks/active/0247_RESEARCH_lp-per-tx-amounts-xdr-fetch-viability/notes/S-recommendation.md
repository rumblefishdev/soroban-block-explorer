---
prefix: S
title: Preliminary recommendation + task reframe (pre-benchmark)
status: draft
spawned_from: '0247'
---

# Preliminary synthesis — pre-benchmark

**Status:** draft. The Path-A latency benchmark (the original empirical gate,
`Q-s3-archive-fetch-latency.md`) is NOT yet run — no AWS network access in
this session. This note records the design findings that already shift the
recommendation before the numbers land.

## Finding 1 — the real cost is a result-meta diff, not S3 latency

See `R-xdr-amount-source.md`. Actual LP amounts are not in the op body; they
require parsing the transaction **result meta** (pre/post `LiquidityPool`
reserve deltas) and, for trades, the **operation result** (which
`xdr-parser` does not expose today — new extractor needed). This parse is
**path-neutral work**: identical whether run at read time (A) or ingest
time (C).

Consequence: the task is no longer "S3 fetch latency vs DB read". It is
"run the same XDR result-meta parse **once at ingest** (C) or **repeatedly
at read time** (A)". For a popular pool's transaction list — a polled,
paginated, hot read path — A re-does the same parse on every page view.

## Finding 2 — ADR 0029's rejection does NOT bind Path B/C

ADR 0029 Alt 2 ("serve everything from DB") was REJECTED because
"DB size explodes" — but that rejection is about mirroring the **entire
heavy corpus** (memo blobs, signatures, full XDR envelopes, event
topics/data across tens of millions of ledgers = multi-TB). The ADR's
primary driver is verbatim "Reducing S3 storage cost".

Path B/C stores **only LP-op amounts** — per the task README, LP ops are a
tiny subset (~tens of thousands of rows total across all mainnet history,
~80 bytes each = single-digit MB). That is three-plus orders of magnitude
off the corpus the ADR rejected. **ADR 0029 does not actually prohibit a
narrow LP-amount side table** — it prohibits re-mirroring the whole archive.
A short ADR amendment can carve this out explicitly without reopening the
core decision.

This conclusion holds independent of ClickHouse, but CH-primary reads
(now live for 5 modules) reinforce it: a narrow append-only
`lp_operation_amounts` table is cheap in CH's columnar layout, and the
indexer already does a full XDR pass per ledger (task 0163 fold-count), so
the marginal ingest cost of extracting LP reserve deltas in the same pass
is low.

## FINAL 2026-06-03 — verdict: Path C (product requires 100%)

Gate closed by measurement + product decision:

- Measured per-op collision rate = **25%** (prod CH). Path E is exact only
  for the ~75% non-colliding rows.
- Product decision: per-tx amounts are **must-have 100%** (Figma column).
- 25% rows is too much for a Path-A XDR hot-path fallback.

→ **Selected path: C** — extract per-op LP amounts at ingest from each
operation's own (non-collapsed) `LedgerEntryChanges`. 100% per-op, no
collision, no hot-path S3. Path E is dropped (its 75% is a strict subset of
C's coverage; building C makes E redundant). Path A dropped. Path D was
interim only.

The path decision feeds the existing **task 0279** (FEATURE, from 0274). Research task
0247 conclusion reached.

The Path-E-vs-C discussion below is retained for history.

## UPDATE 2026-06-03 — Path E supersedes earlier Path-C lean (now itself superseded by FINAL above)

After this note was first drafted, a CH-schema check surfaced **Path E**
(pure SQL over `liquidity_pool_snapshots` reserve deltas — see
`R-clickhouse-snapshot-delta.md`). Path E beats both A and C on effort (no
XDR fetch, no new table, no extractor, no ADR amendment) **for ledgers with
one LP op per pool**. Revised ordering:

1. **Path E** — pure CH SQL, primary, gated on the collision-rate measurement.
2. **Path A** — read-time XDR, now the **fallback** for colliding ledgers only.
3. **Path C** — escalation if collisions dominate hot pools (true per-op at
   ingest, no collision problem).
4. **Path D** — MVP interim, unblocks FE 0077 now.

**Revised sequencing:** ship **D** now → implement **E** as the real
solution once E1 (collision rate) + E2 (delta validation) pass → wire **A**
as the collision fallback → keep **C** in reserve. The Path-C-first lean
below is retained for history but is superseded by Path E.

## Original lean (superseded by Path E): Path C (ingest-side extraction)

Ordering of paths by current evidence:

1. **Path C** — extract LP amounts during the indexer's existing XDR pass,
   write a narrow `lp_operation_amounts` side table (tx_id, op_index,
   pool_id, amount_a, amount_b, direction). API reads it directly. No
   hot-path S3, no per-request re-parse. Cost: indexer change + backfill +
   short ADR 0029 amendment.
2. **Path A** — read-time fetch. Now less attractive: per-request cost is a
   full result-meta diff (heavier than the single-tx E3 extract the latency
   note assumed), paid repeatedly on a hot list endpoint. Still the fastest
   to ship (infra exists). Viable as an **interim** behind `?expand=`.
3. **Path B** — same storage as C but coupling amount columns onto
   `operations_appearances`; rejected vs C for schema cleanliness (C keeps
   a separate LP-only table).
4. **Path D** — ship without amounts (current `PoolTransactionItem` shape).
   Zero backend work; unblocks FE 0077 MVP now.

**Recommended sequencing:** ship **D** as the MVP interim (FE 0077 not
blocked), pursue **C** as the proper solution. Use **A** only if the
benchmark shows read-time cost is trivial AND the team wants to avoid an
ADR amendment — but the result-meta-diff finding makes that unlikely on a
hot path.

## What still gates a final recommendation

- [ ] Run the Path-A benchmark (`Q-s3-archive-fetch-latency.md`) — measure
      the **full result-meta diff** cost (not the lighter E3 extract), p50/
      p95/p99, warm + cold, 20-row page / ~8 ledgers, from the API's region.
- [ ] Confirm `xdr-parser` op-result extractor scope for trade direction
      (new surface, not currently exposed).
- [ ] Count LP ops in full mainnet history → confirm the "~tens of
      thousands rows" storage claim (backfill sizing for C).
- [ ] Team / ADR-author sign-off on a narrow ADR 0029 amendment for C.
