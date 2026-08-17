---
id: '0463'
title: 'FEATURE: account detail — show zero-balance trustlines + signers/thresholds'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0464', '0321', '0331', '0295', '0214']
tags:
  [
    frontend,
    backend,
    account-detail,
    clickhouse,
    soroban-rpc,
    priority-medium,
    effort-medium,
  ]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/377']
history:
  - date: '2026-08-04'
    status: backlog
    who: karolkow
    note: >
      Triaged from issue #377 (two asks in one report, both on the account
      detail page). Claims verified against prod ClickHouse and Horizon
      before filing.
  - date: '2026-08-04'
    status: backlog
    who: karolkow
    note: >
      Converted to a directory after a 200-account measurement (notes/R-)
      and a source comparison (notes/S-). Three things changed: the feature
      matters more than assumed (97.8% of hidden rows on typical accounts
      are live trustlines), Horizon is excluded from the runtime path by
      decision, and an earlier claim of mine — that signers can never be
      indexed — was wrong and is corrected in notes/S-. Design is NOT final:
      the signers/balances question is being re-researched in a fresh
      session.
  - date: '2026-08-17'
    status: active
    who: karolkow
    note: >
      Activated. Opened with the design still unsettled on purpose — the
      solution space is being re-planned from scratch before any code, since
      option A was chosen for cost rather than fit and the signers half may
      belong to a different option entirely.
---

# FEATURE: account detail — zero-balance trustlines + signers/thresholds

## Summary

Two gaps on the account detail page, reported together in issue #377:

1. **A trustline that exists but holds 0 is invisible.** The read path drops
   it. An established trustline at zero is a real fact — the account can
   receive that asset.
2. **Signers and thresholds are not shown at all.** Nothing on the page says
   whether an account is multisig. We do not index this today.

## The trap that makes it non-trivial

A **removed** trustline is written as `amount = 0` too
(`persist/stage.rs:33-40`, write site `:1686`), so the two are byte-identical
in ClickHouse. Deleting the `amount != 0` filter resurrects closed trustlines
as ghosts — the inverse of the merged-account ghosts in task 0321.

Stellar's own docs confirm this is structural, not accidental: removing a
trustline **requires a zero balance** (`CHANGE_TRUST_INVALID_LIMIT` —
"attempting to remove a trustline with a non-zero asset balance"). Every
closed trustline passed through zero on its way out.

## Fixture and evidence

Reported account `GDXWIA4VF3GW2R5OSVIROD47W6AQHE33DSEG6TF7YZD3DYOVU54MYBEN`:
five rows in `balances`, two rendered. AQUA / SHX / USDC sit at 0 and Horizon
confirms all three trustlines are live. The same account carries 5 ed25519
signers at weight 1 with thresholds 3/3/3 — genuinely multisig, presented by
us as ordinary.

Filter: `crates/api/src/accounts/queries.rs:422`.

Measurements and the full option comparison live in the notes:

- [`notes/R-zero-balance-probe.md`](notes/R-zero-balance-probe.md) —
  200-account probe, bimodal distribution, the 33.6 M ambiguous-pair count.
- [`notes/S-source-options.md`](notes/S-source-options.md) — every source
  considered, what is ruled out and why, and two corrections to earlier claims.

The earlier read-time-RPC design in `notes/S-` is **superseded** — it cannot
reach backward completeness, because `getLedgerEntries` has no enumeration
primitive. Keep the notes as the record of how the decision was reached; do
not implement from them.

Planning map (local, gitignored, not in the repo): `.wayfinder/0463/` — the
decision trail, five resolved research tickets with their measurements, and
the implementation tickets.

## Decided design — [ADR 0055](../../../2-adrs/0055_holding-lifecycle-column-on-balances.md)

The lifecycle becomes a **column on `balances`**; rows are never deleted.

```sql
ALTER TABLE balances ADD COLUMN closed_at_ledger Int64 DEFAULT 0;
```

The entity is the **holding relationship**, not the trustline: the same
ambiguity affects Soroban and LP holdings, and task 0331 already unified every
holding kind into this one table. The read path filters on
`closed_at_ledger = 0` instead of `amount != 0`.

Signers take a side table `account_signers` (single writer, RMT by ledger) —
not a column on `accounts`, whose whole-row replacement makes a bolt-on unsafe.

Backward completeness comes from a one-off seed of the history archive's
checkpoint bucket list (**4.54 GB gzipped, 21 files**, measured), which both
fills the ~7 % we hold no row for **and** derives the closures as
`{our zero rows} − {live in snapshot}`. Without that second step, flipping the
filter resurrects ghosts.

Full reasoning, rejected alternatives and the measured production facts are in
the ADR.

## Work breakdown

1. **Lifecycle column + writer** — `ALTER` with `DEFAULT`, then the writer,
   covering classic, Soroban and LP write paths together (deferring any kind
   costs a second full backward pass). Must also stamp the **account-removal**
   path (`state.rs:426-449`), so a merged account's native tombstone carries
   `closed_at_ledger` and native needs no special case downstream. Deployment
   order is load-bearing — task 0310.
2. **Signers extraction** — parallel to the above, no seed needed; the dormant
   set is empty (0 of 123,772 measured).
3. **Seed from the checkpoint snapshot** — fills gaps and marks closures.
   Version on each entry's own `lastModifiedLedgerSeq`, never on a window
   boundary (task 0492).
4. **Flip the read filter + production verification** — only after the seed
   verifies. This is where **native** becomes visible too (239,087 holders):
   under `closed_at_ledger = 0` it needs no exemption of its own. A standalone
   native read-filter patch was built and reverted on purpose — it would have
   been a temporary special case dissolved by this very step.

## Scope

**In:** classic, native and Soroban holdings; the LP **write path**; signers
and thresholds.

**Out:** rendering LP positions on the account page — task 0493, because the
page renders none today and `lp_positions` is ordered `(pool_id, account_id)`,
making the account-side read a full scan. Balance history over time — task 0464.

**Under investigation, non-blocking:** Soroban entries have an
archived-but-restorable state the codebase never reads, so type-3 holdings may
over-report.

## Acceptance criteria

- [ ] A live zero-balance trustline appears; the fixture account shows five
      assets, not two
- [ ] A **closed** trustline still does not appear — verified on an account
      with a known removal, not only the happy path
- [ ] The 873-zero-row account shows none of those 873
- [ ] Native zero balances appear (239,087 holders), watching the two-convention
      trap for native
- [ ] Signers (key, weight, type) and low/med/high thresholds are shown; the
      fixture reads as multisig
- [ ] `total_supply` and `holder_count` unchanged for a spot-checked asset
- [ ] The 200-account probe from `notes/R-` returns zero accounts where the
      chain holds more live zero trustlines than we do
- [ ] **Verified on production**, not at merge — the destination is a shipped,
      checked change
- [ ] **Docs updated** — `docs/architecture/**` read path and frontend data
      contract; `docs/backfills.md` gains the seed pass
- [ ] **API types regenerated** — yes, the account DTO gains fields
      (`npx nx run @rumblefish/api-types:generate`)
