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

## Current design — A, read-time Soroban RPC

Not final; see the open question at the end of `S-`.

One batched `getLedgerEntries` per account-detail request:

- `LedgerKey::Account` → the full `AccountEntry`, **signers and thresholds
  included**;
- `LedgerKey::Trustline` for the account's zero rows → **entry present = live,
  absent = closed**. We never enumerate; ClickHouse names the candidates.

Four things the measurement added to the naive version:

1. **No call when the account is merged.** 58 % of the sampled population. The
   page already derives `deleted`; a merged account has nothing to verify.
2. **Batch in parallel, cap only as a backstop.** One sampled account carries
   873 zero rows — five sequential round-trips on a page view. Parallel
   batches make that one round-trip's latency; keep a high cap for absurdity.
3. **Failure is stated, not silent.** Hiding all zeros on a failed lookup is a
   return to the bug being fixed. Say the verification did not run.
4. **Use the RPC pool with failover** (`SOROBAN_RPC_URLS`, as `nft_token_uri`
   does) plus a short per-account cache — not a single hardcoded endpoint.

### Accepted gaps

- **~7 % of live zero trustlines stay invisible** — we hold no row for them,
  and `getLedgerEntries` cannot list an account's trustlines. Closing this
  needs the database route (task 0464).
- Type-3 (Soroban token) holdings hit the same zero ambiguity, but their
  existence question is a `ContractData` entry, not a trustline. Classic only
  in the first cut.
- `balance_aggregates_mv` computes `holder_count` as `countIf(amount > 0)`.
  Nothing here changes that and it must stay so: a zero-balance trustline is
  not a holder.

## Prerequisite

The `getLedgerEntries` client, key builders and decoders live in
`crates/backfill-runner/src/rpc_snapshot.rs`. Its own module docs pre-authorise
the move: _"the refactor-to-a-shared-crate is a one-day move if a second
consumer appears."_ The API is that consumer. Move it beside
`crates/enrichment-shared/nft_token_uri`, which already speaks Soroban RPC.
`AccountSnapshot` (`:519`) is deliberately lean and discards
signers/thresholds/flags — widen it.

## Acceptance criteria

- [ ] A live zero-balance trustline appears; the fixture account shows five
      assets, not two
- [ ] A **closed** trustline still does not appear — verified on an account
      with a known removal, not only the happy path
- [ ] A merged account triggers no RPC call at all
- [ ] Signers (key, weight, type) and low/med/high thresholds are shown; the
      fixture reads as multisig
- [ ] A failed lookup says so on screen and hides nothing silently
- [ ] The moved RPC client keeps `backfill-runner` green
- [ ] **Docs updated** — `docs/architecture/**` read-path / frontend data
      contract sections, since the account-detail response shape changes
- [ ] **API types regenerated** — yes, the account DTO gains fields
      (`npx nx run @rumblefish/api-types:generate`)

## Relation to 0464

Task 0464 (trustline as an entity) would replace this task's **trustline**
half and, per the correction in `notes/S-`, could replace the signers half
too. It is gated on an archive re-parse, so it is not a substitute today.
