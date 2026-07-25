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

> **Status 2026-07-22 — items 1 and 2 are both resolved, and both were resolved
> differently than this task predicted. Read the two notes below before working
> either.**

### 1. ~~Stale RDS docs~~ — DONE by task 0248, and the premise here was wrong

Task **0248** swept both documents on 2026-07-22. Nothing is left in item 1.

But the framing below — "0239 decommissioned RDS … teardown happened" — is
**false, in the same way the docs were false**. Per the 0249 archive, **zero
production stacks were ever deployed** (`validateConfig` blocked on
`hostedZoneId: "CHANGE_ME"`), so 0239's Phase 6 closed **vacuously**: there was
no production RDS to tear down. Only a _staging_ RDS ever ran, destroyed
2026-05-21. The prose did not describe a decommissioned deployment — it
described a design that was never built. Left in place below as written, so the
correction is visible rather than silently overwritten.

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

> **Resolved 2026-07-22 the other way — 0214 was moved back to `active/`, not
> marked completed. This overrides the decision recorded above, so here is the
> evidence, and reverse it if you disagree.**
>
> The instruction above rests on "its Phase-1/2 shipped; Phase-3 is carved out".
> Phase-1/2 has **not** shipped to production. Measured on prod before deciding:
>
> - the sequence-number criterion **is** met — 91.4% (13,134,062 of 14,364,747)
>   against a >50% bar
> - the E06 witness account `GARDNV3Q7…` is **not** fixed: `sequence_number = 0`,
>   `home_domain` NULL, zero balance rows
>
> I first suspected that account was the "one genuine non-participant skeleton
> edge case" 0214's own history says was accepted. It is not — it has
> **903,373,913** appearances in `transaction_participants` and spans ledgers
> 50,457,424 → 63,600,904. It is one of the busiest accounts on the network.
>
> Reconciling with 0214's history, which reported all three empirical criteria
> satisfied: that run was verified on a **512k-ledger pilot window**, where it
> genuinely cut the skeleton rate 17.21% → 0.80%. It was never applied across
> full production history. So 0214 is blocked on a backfill window, not carved
> out — and marking it completed would have buried that.
>
> Its third criterion is separately unsatisfiable: it targets
> `account_balances_current`, now a dead table (0 rows, no writer, no live
> reader; the live one is `balances` at 89,634,237 rows). Flagged in 0214 for
> rewriting, and the dead table flagged to 0310.

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

- [x] Item 1 — stale RDS docs rewritten to CH-on-Hetzner. **Done by task 0248,
      2026-07-22.** The "no live RDS" wording was itself too generous: there was
      never a live production RDS to remove.
- [x] Item 2 — 0214 status/location reconciled, **by moving it to `active/`, not
      by marking it completed.** Evidence in the note under item 2; reverse it if
      you disagree with the call.

      > Credit where due: item 3 below **already recorded** that
      > `account_balances_current` was retired by 0331 and that balances now land
      > in `balances`. I re-derived that from prod today and briefly wrote it up
      > as a new finding before spotting it here. It is not new — the only new
      > part is the measurement (0 rows vs 89,634,237) and that 0214 still
      > carries it as an acceptance criterion.

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
