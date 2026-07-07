---
id: '0362'
title: 'Post-0244 loose ends: stale RDS docs, 0214 mis-archive, Phase-3 trustline pointer, galexie metadata validation'
type: REFACTOR
status: backlog
related_adr: ['0032', '0051']
related_tasks: ['0244', '0239', '0214', '0304', '0310', '0331', '0339']
tags: [cleanup, docs, follow-up, lore-hygiene, priority-low, effort-small]
links: []
history:
  - date: 2026-07-07
    status: backlog
    who: karolkow
    note: >
      Spawned to give the loose ends from the 0244 PG-removal session a home
      (so none are silently skipped). Four unrelated small items bundled per the
      "don't micro-decompose" convention. Each is independently pickable; none
      blocks anything. Phase-3 trustline is tracked here as a POINTER only — it is
      a design-gated feature, not do-able in this task.
---

# Post-0244 loose ends

## Summary

A grab-bag of small, unrelated follow-ups surfaced during the 0244 PG-removal
session. Bundled into one backlog task so each has a discoverable home rather
than living only in a commit message or an archived task body. None is urgent;
none blocks anything. Pick individually.

## Items

### 1. Stale RDS docs (0239 done → docs describe decommissioned infra)

Task 0239 (AWS cutover) decommissioned RDS: Lambdas out-of-VPC, mTLS to Hetzner,
RDS + NAT GW torn down. But two docs still describe RDS as live/frozen-pending:

- `docs/architecture/**/infrastructure-overview.md` — RDS topology.
- `docs/architecture/**/technical-design.md` §infra — carries a banner
  "teardown scheduled task 0239"; 0239 is **completed**, so the banner is false
  (teardown happened).

Fix = rewrite both to the current CH-on-Hetzner reality (no RDS). These were held
during the 0244 doc sweep on the assumption 0239 was still pending; it wasn't.

**AC:** `- [ ]` `infrastructure-overview.md` + technical-design §infra describe
CH-on-Hetzner, no live RDS, no "teardown pending" banner.

### 2. 0214 mis-archived (file in `archive/` but `status: active`)

`lore/1-tasks/archive/0214_...md` sits in `archive/` yet its frontmatter says
`status: active`, and it still carries a pending Phase-3 (trustline). Either the
file or the status is wrong. Decision recorded in the 0244 session: Phase-3 will
be done elsewhere, so 0214 should NOT return to `active/`. Cleanest resolution:
mark 0214 `status: completed` (its Phase-1/2 shipped; Phase-3 is carved out — see
item 3) so location and status agree, and move the Phase-3 pointer to item 3 here.

**AC:** `- [ ]` 0214 frontmatter `status` matches its `archive/` location
(completed), with a history note that Phase-3 is carved to this task.

### 3. Phase-3 trustline pass — POINTER ONLY (design-gated, do NOT implement here)

The parked trustline scaffolding in `crates/backfill-runner/src/rpc_snapshot.rs`
(`trustline_ledger_key`, `decode_trustline_snapshot`, `rebuild_trustline_asset`,
`Trustline*` types — all `#[allow(dead_code)]`, unit-tested) is Phase-3 of 0214.
It is **deferred, not skipped**. Why it is not a wire-up:

- **`collect_trustline_candidates` is a phantom** — referenced in a comment
  (`rpc_snapshot.rs:484`) but never written. The candidate-pairing IS the hard
  part: Soroban RPC has no "list trustlines for account" primitive, so keys must
  be enumerated as (account × asset) pairs. Naive overshoot = billions of keys at
  mainnet scale (~60k SAC-assets × ~100k accounts); "observed assets" pairing is
  circular (bootstrap targets exactly the UN-observed accounts).
- **Target table changed** — 0214 §Phase-3 says "populate `account_balances_current`",
  but 0331 (Option A) retired that write path; native/trustline balances now land
  in the unified `balances` table. The spec premise is stale.
- **Asset-aggregate half already done** — `balance_aggregates_mv` over `balances`
  computes supply/holders (0331/0339), so that half of Phase-3 is obsolete.

**Not actionable until someone decides the pairing strategy.** When that decision
exists, spawn a dedicated FEATURE task (this is not it) and repoint the code
comments away from archived 0214.

**AC:** `- [ ]` No code change. Either: a pairing strategy is decided + a dedicated
FEATURE task spawned, OR a decision to permanently drop trustline-balances is
recorded and the parked code removed. (Tracking only — closing this item = making
that call, not implementing.)

### 4. Galexie created-vs-updated metadata validation (0304 nice-to-have)

0304 carried an open non-blocking validation: direct created-vs-updated
confirmation of a `soroban_contract_metadata` write on a representative deploy via
the galexie archive, cross-checking decimals/symbol/name against an independent
source. Manual/ops (needs archive access). Carried here so it is not lost.

**AC:** `- [ ]` One representative deploy's metadata cross-checked vs an
independent source, OR the check is explicitly declined as not worth the effort.

## Acceptance Criteria

- [ ] Item 1 — stale RDS docs rewritten to CH-on-Hetzner (no live RDS).
- [ ] Item 2 — 0214 status/location reconciled (completed).
- [ ] Item 3 — Phase-3 pairing decided + dedicated task spawned, OR trustline
      code dropped by decision. (Tracking only — no code here.)
- [ ] Item 4 — galexie metadata cross-check done or explicitly declined.
- [ ] **Docs updated** — item 1 IS the doc update; items 2–4 are lore/ops, N/A
      for `docs/architecture/**` beyond item 1.
- [ ] **API types regenerated** — N/A (no `crates/api/**` change).

## Notes

- All four are independent; do or decline each on its own merit.
- Origin: the 0244 PG-removal session (2026-07-07). See commits `e10c5065`
  (0304 archive), `715139d2` (0310 sync), `79da1f39`/`2530fd8a` (0350 Nit 4).
