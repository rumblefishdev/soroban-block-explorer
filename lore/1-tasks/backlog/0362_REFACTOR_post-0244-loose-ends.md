---
id: '0362'
title: 'Post-0244 loose ends: stale RDS docs, 0214 mis-archive, Phase-3 trustline pointer, galexie metadata validation'
type: REFACTOR
status: backlog
related_adr: ['0032', '0051', '0055']
related_tasks:
  ['0244', '0239', '0214', '0304', '0310', '0331', '0339', '0463', '0502']
tags: [cleanup, docs, follow-up, lore-hygiene, priority-low, effort-small]
links: []
history:
  - date: '2026-08-19'
    status: backlog
    who: karolkow
    note: >
      Items 3 and 4 closed; all four items now resolved. Item 3 — the RPC
      trustline route is dropped (0463 measured the same wall: getLedgerEntries
      has no enumeration primitive), the work moves to ADR 0055 + the
      checkpoint-bucket seed in 0502. Removed trustline_ledger_key,
      rebuild_trustline_asset and their 2 tests (rpc_snapshot.rs 1004 -> 899
      lines); kept decode_trustline_snapshot as a pure LedgerEntryData decoder
      the checkpoint route reuses. cargo check clean, 53 tests pass. Item 4 —
      metadata created-vs-updated confirmed on prod (2,669 written at the deploy
      ledger, 1,227 later, 0 impossible, of 3,896 contracts) and name/symbol/
      decimals cross-checked against on-chain instance storage for 3 contracts,
      3/3 exact; used live Soroban RPC rather than the galexie archive, stated
      in the item. Also corrected item 2's note: 0214 was moved back to
      archive/completed on 2026-07-23 (4336250d), reversing the day-old call the
      note described.
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

> **Reversed again the next day, 2026-07-23 — and that is where it stands.**
> The task-currency sweep (`4336250d`) moved 0214 back to `archive/` with
> `status: completed`. Verified on `develop` today (2026-08-19):
> `lore/1-tasks/archive/0214_FEATURE_ch-initial-snapshot-account-state.md`,
> `status: completed`. So the note above describes a state that lasted one day.
>
> The sweep's reason is not the one this item argued about: 0214's deliverable
> — the backfill-start snapshot mechanism — shipped in PR #189, and the
> residual skeleton accounts that looked like a 0214 gap are **0421's
> first_seen/sequence_number clobber**, proven by decoding raw
> `tx_changes_before`. Same evidence (`GARDNV3Q7…` still broken), different
> owner. Item 2's acceptance criterion — location and status agree — is
> satisfied either way; only the prose above was stale.

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

**AC:** `- [x]` **Decided 2026-08-19 — the RPC route is dropped, and its
half of the parked code with it. Trustline balances are NOT dropped; they
arrive by a different route.**

The call was not a preference. Task 0463 settled the same question
independently, with a 200-account probe: the read-time-RPC design is
**superseded because `getLedgerEntries` has no enumeration primitive**, so it
cannot reach backward completeness. That is the same wall this item recorded as
"`collect_trustline_candidates` is a phantom" — the pairing IS the problem, and
it has no solution on the RPC path: the pairs you cannot guess are exactly the
ones missing from our stream.

What replaces it:

- **Lifecycle** — [ADR 0055](../../2-adrs/0055_holding-lifecycle-column-on-balances.md):
  `closed_at_ledger` on `balances`, rows never deleted, read path filters on it
  instead of `amount != 0` (task 0463).
- **Backward completeness** — a one-off seed from the SDF history archive's
  checkpoint bucket list (task 0502), which publishes full pubnet state as raw
  XDR. Enumeration is not needed there: the buckets already contain every
  `TrustLineEntry`.

**Code removed** (`crates/backfill-runner/src/rpc_snapshot.rs`, 1,004 → 899
lines):

| symbol                                                                       | why it went                                                                    |
| ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `trustline_ledger_key`                                                       | builds a `LedgerKey::Trustline` for an RPC request — the request is never made |
| `rebuild_trustline_asset`                                                    | reverses key → asset for the RPC round trip only                               |
| `rebuild_trustline_asset_alphanum4_round_trip` + `..._rejects_oversize_code` | covered only the removed function                                              |
| `LedgerKeyTrustLine` import                                                  | unused after the above                                                         |

**Code deliberately kept**, with its comment repointed away from archived 0214:
`decode_trustline_snapshot` and the `Trustline{Snapshot,Asset,AssetType}` types.
These are a **pure decoder of `LedgerEntryData::Trustline`** — not RPC-shaped —
and that is exactly the shape a checkpoint bucket entry carries, so 0502 reuses
them rather than rewriting them. Deleting them would have been the tidier-looking
call and the more expensive one.

Verified: `cargo check -p backfill-runner --all-targets` clean (0 warnings),
`cargo test -p backfill-runner` 53 passed / 0 failed, and no reference to the
removed symbols survives outside archived lore.

### 4. Galexie created-vs-updated metadata validation (0304 nice-to-have)

0304 carried an open non-blocking validation: direct created-vs-updated
confirmation of a `soroban_contract_metadata` write on a representative deploy via
the galexie archive, cross-checking decimals/symbol/name against an independent
source. Manual/ops (needs archive access). Carried here so it is not lost.

**AC:** `- [x]` **Done 2026-08-19 — cross-checked, 3 of 3 exact, both write
paths confirmed on production.**

**Created vs updated — both paths are live.** Comparing each contract's first
`soroban_contract_metadata` version against its deploy ledger, on production:

| first metadata write                           | contracts |
| ---------------------------------------------- | --------- |
| at the deploy ledger (`created` change)        | 2,669     |
| at a later ledger (`updated` / `state` change) | 1,227     |
| before the deploy ledger (impossible)          | 0         |
| no matching `soroban_contracts` row            | 0         |
| **total**                                      | **3,896** |

So both branches of `entry_token_metadata`
(`crates/xdr-parser/src/ledger_entry_changes.rs:143,147`) are exercised by real
traffic, and no row is stamped earlier than the contract it belongs to. The
parser-level halves already had unit cover
(`contract_instance_metadata_extracted_on_created` / `..._on_updated`); this is
the production counterpart they lacked.

**Value cross-check — against chain state, not the archive.** Read each
contract's instance storage `Symbol("METADATA")` map live from a public Soroban
RPC node (`stellar contract read --id … --rpc-url https://mainnet.sorobanrpc.com`,
CLI 26.0.0) and compared it to what we store:

| contract        | cohort    | our `name` / `symbol` / `decimals`           | on-chain  | match |
| --------------- | --------- | -------------------------------------------- | --------- | ----- |
| `CA26OJGB…F2F2` | at deploy | 蛇年欢庆 (Snake Year Celebration) / SMOL / 0 | identical | ✅    |
| `CA25XTGH…R2KQ` | later     | DeFindex-Vault-TurboTestVaultD2 / TTVD / 7   | identical | ✅    |
| `CCAOOEX2…U3F`  | later     | Splyce Concord Share / sUSDC / 7             | identical | ✅    |

Two findings worth keeping:

- **The map key for decimals is not stable across contracts** — `decimal`
  (DeFindex) vs `decimals` (the other two). Both decoded correctly, so
  `extract_token_metadata` already handles the pair; nothing to fix, but any
  future rewrite of that decoder must keep both spellings.
- **Metadata mutation is unobserved on production.** Only 6 of 3,896 contracts
  carry more than one row, and in every one of those the name / symbol /
  decimals are byte-identical across versions — i.e. RMT copies of the same
  value (see 0420), not a value that changed. There is currently no real
  overwrite to test the update path's _correctness_ against.

**Deviation from the AC as written, stated plainly:** the AC named the galexie
archive; this used live RPC instance state instead. The archive would prove
"what was written at that ledger"; RPC proves "what the chain says now". They
coincide only while the value has not changed since — which the bullet above
measures as true for every contract we hold. Good enough to close the item; if
someone later needs the stronger claim, it needs the archive read, not this.

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

- [x] Item 3 — **decided 2026-08-19: the RPC pairing route is dropped**, its
      code removed, the pure decoder kept for the checkpoint route (ADR 0055 +
      task 0502). No new task spawned: 0463 and 0502 already own the work, and
      spawning a third would duplicate them.

      > Deviation from the AC as written: it said "no code change here". The
      > decision made ~105 lines provably unreachable, so they went in the same
      > pass rather than being left for a future reader to re-derive. The AC's
      > second branch ("the parked code removed") anticipated exactly this.

- [x] Item 4 — **galexie metadata cross-check done 2026-08-19**, 3 of 3 exact,
      both write paths confirmed on production (2,669 at deploy / 1,227 later).
      Substituted live RPC instance state for the archive read — stated in the
      item.
- [x] **Docs updated** — item 1 was done by 0248;
      `docs/architecture/database-schema/clickhouse-pilot.md` §"What this does
      not cover (yet)" rewritten under item 3 (it advertised
      `rebuild_trustline_asset` as "ready to wire in"). Items 2 and 4 are
      lore/ops — `N/A` for `docs/architecture/**`.
- [x] **API types regenerated** — N/A (no `crates/api/**` change).

## Notes

- All four are independent; do or decline each on its own merit.
- Origin: the 0244 PG-removal session (2026-07-07). See commits `e10c5065`
  (0304 archive), `715139d2` (0310 sync), `79da1f39`/`2530fd8a` (0350 Nit 4).
