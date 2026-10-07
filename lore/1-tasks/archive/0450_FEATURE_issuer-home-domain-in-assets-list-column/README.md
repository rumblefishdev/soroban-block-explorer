---
id: '0450'
title: 'FEATURE: show the issuer home domain in the assets-list issuer column (already fetched, dropped at serialisation)'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0319', '0334', '0364', '0371', '0469', '0630']
tags: [backend, api, frontend, assets, priority-medium, effort-medium]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/369'
history:
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Spawned from external feedback on the live deployment: when searching an
      asset by code, show the issuer name alongside the address in the issuer
      column — `G… - Centre.io`. The requested value is the issuer's
      **home domain**, not an organisation name, and the list path already
      fetches it for every row; it is discarded before serialisation. Separate
      feedback stream from the batch that produced 0440-0445.
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Correction on review — the premise "we do not show issuer domains for
      assets" was too broad. The asset **detail** page already renders a
      `Domain` row, derived from the SEP-1 `DOCUMENTATION.ORG_URL` hostname
      (`web/src/pages/assets/AssetMetadata.tsx:38-41`), which is a *different*
      source from the accounts list's on-chain `home_domain`. Only the assets
      **list** shows nothing. That turns this from "plumb a field through" into
      "pick one meaning of Domain and make both surfaces agree" — the two
      sources can disagree, and the list cannot use the SEP-1 one because it is
      a per-request `stellar.toml` fetch. Scope rewritten; a recorded source
      decision is now the first acceptance criterion.
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Second finding, larger than the first: **the target column does not show
      an issuer for the assets that matter.** "Issuer / Contract ID" is
      polymorphic and its `sac_contract_id` branch is evaluated *before* the
      `issuer` branch (`web/src/pages/assets/AssetsTable.tsx:75-88`), so any
      classic asset with an observed SAC renders a `C…` contract address and
      never its `G…` issuer. USDC — the example in the report — has a SAC, so
      the row the reporter looked at shows neither the issuer nor the domain
      she asked for. The chip is meaningless until the column question is
      answered, so that decision is now a criterion too. Effort raised
      small → medium: this is no longer one field and one cell.
  - date: '2026-07-28'
    status: active
    who: karolkow
    note: >
      Implemented. Both open decisions taken and recorded in code comments so
      they cannot be silently reverted. **Source:** the list uses the on-chain
      `home_domain`; the detail page's SEP-1-derived `Domain` row is untouched
      for now, so the "one meaning of Domain" cleanup is still owed — the two
      can still disagree, they just no longer contradict on the same surface.
      **Column:** `issuer` now precedes `sac_contract_id`, so a wrapped classic
      asset shows its issuer again; nothing is lost because the Token column
      already carries its own `SAC` chip. Width 160 → 240, matching accounts.
      The accounts-list chip was lifted into `libs/ui` as `DomainChip` and both
      tables now render from it rather than two copies. Two regression tests
      added (`web/src/pages/AssetsListPage.test.tsx`): a SAC-wrapped classic
      asset shows issuer + chip and NOT the SAC address, and an issuer with no
      domain produces no outbound link. web 119 green, ui 76 green, `cargo
      check -p api` clean, API types regenerated (additive only). Verified
      visually against a local stub, since the deployed API has no such field
      yet. NOT deployed — the issue stays open until it is.
  - date: '2026-07-28'
    status: active
    who: karolkow
    note: >
      Two corrections to the entry above, both from review.
      **(a) "Nothing is lost by the reorder" was wrong.** The SAC address is no
      longer on the list at all: the Token column's `SAC` chip is a plain label,
      not a link, and it only appears when `sac_deployed`, so a reserved
      un-deployed SAC now shows nowhere on the list. The address is on the asset
      detail page (`web/src/pages/assets/AssetSummary.tsx:102-130`),
      untruncated and copyable — so it is one click away rather than gone, but
      directness was lost and the commit message overstated it. Accepted: the
      alternatives are a crowded cell, or linking the `SAC` chip, which would
      make one chip navigate while the identical-looking `Classic`/`Soroban`
      type badges do not — those are categories with nowhere to point (ADR 0051
      keeps the SAC facet deliberately orthogonal to the type axis).
      **(b) Branch order hardened.** `issuer` is now tested FIRST, not second.
      Both orderings work today because `issuer_id` is 0 for native and
      soroban-native (`crates/db-clickhouse/schema/init.sql:299`), but testing a
      contract column first puts the original trap one refactor away — merging
      `contract_id` and `sac_contract_id` would silently displace the issuer
      again. Issuer-first is the only branch whose condition does not depend on
      the contract columns. A third regression test pins the Soroban-native
      fallback.
  - date: '2026-07-28'
    status: active
    who: karolkow
    note: >
      Third pass, prompted by a live example where two adjacent classic assets
      rendered differently for no visible reason. Root cause is one field, not
      several: `sac_contract_id` is set whenever an asset emits a CAP-67
      unified event, which classic transfers do with or without a deployed SAC,
      so it means "has moved" rather than "has a contract". The assets-list
      column and the asset-detail "SAC contract" row both keyed off it; the
      `Has SAC` filter and the `SAC` chip already keyed off `sac_deployed` and
      were right all along. Gated the detail row on `sac_deployed` too, which
      also drops the "Reserved address — not deployed" line — it implied
      somebody reserved the address when in fact every classic asset has one.
      Two assertions added to the existing detail-page tests; verified they
      fail without the gate. Correction to the previous entry: an earlier read
      claimed the filter and the chip disagreed. They do not — that came from
      misreading a hydration query (`queries.rs:508`) as the filter clause.
      New section in the body documents where the handle leaked and where it
      did not, including that the liquidity-pool path is safe only by accident.
  - date: '2026-07-28'
    status: active
    who: karolkow
    note: >
      Reverted the detail-page half of the previous entry on review. Hiding the
      "SAC contract" row for un-deployed SACs was the lossy fix: the row's
      CONTENT was true (here is the address, nothing is deployed at it) and it
      surfaced a genuine oddity — what was wrong is that it appears only for
      assets that have moved. The repair is to make it unconditional, not
      absent, since the address is derivable for every classic asset from
      `(code, issuer, network)` with no query at all. Deferred to 0452 rather
      than widened here. The assets-list column change stands.
  - date: '2026-08-10'
    status: active
    who: karolkow
    note: >
      Audited during the post-deploy verification sweep. The column is LIVE on
      production and renders correctly — verified against real issuers
      (`tokenglade.com`, `damianfi.litemint.store`, `stellarterm.com`), absent
      domains degrade cleanly, both tables share `DomainChip`, the column is
      240 wide, and the wire field plus generated types are in place. Eight
      criteria ticked accordingly; the record previously showed none, which
      read as "nothing shipped".
      Two criteria FAILED and stay open, both found by measuring rather than
      reading. (1) The USDC case the task was written around is EMPTY:
      `issuer_home_domain` is null for
      `GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN`, while the
      account carries `circle.com` on-chain. AQUA (61 863 holders) is the same
      — `aqua.network` on-chain, null for us. Coverage across classic assets
      is 22.2 % (75 288 of 338 963). A whole-row RMT clobber was suspected and
      REFUTED by measurement: of 1 014 072 accounts that ever carried a
      domain, zero lose it on their newest row, so this is missing capture,
      not overwriting. Root cause not yet established — spawned as [[0469]].
      (2) The two-source conflict the task itself predicted was never
      resolved: the list shows the on-chain `home_domain` while the asset
      detail labels a `Domain` row derived from the SEP-1 TOML `ORG_URL`
      hostname (`AssetMetadata.tsx:39`). Same label, different sources, on two
      surfaces describing the same asset.
      Docs criterion stays open until the source question is settled — writing
      the column into the architecture docs while two surfaces disagree on
      what it means would document the ambiguity as if it were the contract.
      The no-extra-round-trip criterion also stays open: it asks for a
      before/after `read_rows` comparison that has not been run.
  - date: '2026-10-06'
    status: completed
    who: karolkow
    note: >
      Archived: the column is live (release #383, issue #369 closed) and the
      three open criteria have owners. USDC stays with 0469 (missing capture).
      One meaning of "Domain" across list and detail, plus the docs criterion
      that waits on it, move to 0630. The round-trip criterion is closed by
      reading, not measuring: `home_domain` is one more column in the issuer
      seek the list already ran per page before this task (`seek_latest_account`,
      tasks 0319 / 0334), so the query count is unchanged.
---

# FEATURE: issuer home domain in the assets-list issuer column

## Summary

Show the issuer's `home_domain` in the assets list, beside the issuer address
(`GA5ZSE… · centre.io`). The value is already read from ClickHouse on the list
path and thrown away at the DTO boundary, so the plumbing is one response field
and one cell change — no new query, no new join, no extra read.

**The plumbing is not the whole job**, and two findings on review moved this out
of "small". The asset detail page already shows a `Domain` from a _different_
source, so the task has to settle which one the product means. And the target
column does not show the issuer at all for a classic asset that has a SAC — the
reporter's own example — so there is nothing to hang the chip on until that is
decided. Both sections below; both decisions come before any code.

Design analysis from July 2026 (why the field was nearly free, the two
sources of "Domain", the column question, reserved SACs, scope, consistency
with the accounts list, constraints):
[notes/R-design-2026-07.md](notes/R-design-2026-07.md).

## Acceptance criteria

- [x] `issuer_home_domain` present on the assets-list item response
- [x] No additional ClickHouse round trip vs today — by reading, not measured:
      the field is one more column of the existing per-page issuer seek
      (`crates/api/src/assets/queries.rs` `seek_latest_account`)
- [x] Source decision recorded (see the two-source section) before coding
- [x] Column decision recorded — a classic asset with a SAC shows its issuer,
      not only the SAC contract
- [ ] Verified on USDC specifically: it has a SAC, so it is the case that is
      broken today and the case the report used (deferred to 0469)
- [x] Column renders `StrKey` + domain; absent domain degrades cleanly
- [x] Issuer column widened (160 → 240) so the chip is not crushed
- [x] Both tables render the cell from one shared component, not two copies
- [ ] Asset detail and assets list agree on what "Domain" means — no surface
      shows a value the other contradicts (deferred to 0630)
- [x] Contract-backed rows (soroban, SAC facet) unchanged
- [ ] **Docs updated** — assets endpoint contract under `docs/architecture/**`
      per ADR 0032 (deferred to 0630, written once "Domain" has one meaning)
- [x] **API types regenerated** — touches `crates/api/**`; run
      `npx nx run @rumblefish/api-types:generate`

## Notes

0371 (asset search by project name / issuer domain) wants the same field as a
_search input_; this task only displays it. Landing this one first gives that
search a visible target to match against.
