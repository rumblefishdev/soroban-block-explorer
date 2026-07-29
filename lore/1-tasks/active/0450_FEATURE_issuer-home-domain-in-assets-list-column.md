---
id: '0450'
title: 'FEATURE: show the issuer home domain in the assets-list issuer column (already fetched, dropped at serialisation)'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0319', '0334', '0364', '0371']
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

## Why it is nearly free

The list resolves its page's issuers through a bloom-pruned `accounts.id`
key-seek rather than a join (0319, 0334), and that seek **already selects
`home_domain`**:

- `crates/api/src/assets/queries.rs:290-304` — `seek_latest_account` selects
  `id, account_id, home_domain`, taking the newest row by `last_seen_ledger`
  because `home_domain` is mutable via `SET_OPTIONS`.
- `crates/api/src/assets/queries.rs:777-790` — the list page resolves all its
  issuers via `resolve_page_issuers`, run concurrently with hydration under one
  `tokio::join!` (0364).
- `crates/api/src/assets/queries.rs:254-277` — `list_row_to_asset_row` maps the
  result onto `AssetRow.issuer_home_domain`, for list rows and detail alike.

Then it stops. `issuer_home_domain` appears nowhere in
`crates/api/src/assets/dto.rs` and nowhere in `libs/api-types/src/openapi.json`;
its only consumer is `crates/api/src/assets/handlers.rs:262`, which uses it
internally to drive the SEP-1 lookup behind the detail page's `description`. The
frontend has never seen the field.

## We already show a domain in two places — and from two different sources

This is not a new UI idea, and that is the complication.

| Surface          | Shows a domain?                                                                             | Source                                                                       |
| ---------------- | ------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Accounts list    | yes, linked chip beside the address (`web/src/pages/accounts/AccountsTable.tsx:30-48`)      | `accounts.home_domain` — **on-chain**                                        |
| Asset **detail** | yes, a `Domain` row plus a `Homepage` link (`web/src/pages/assets/AssetMetadata.tsx:38-41`) | hostname of `home_page`, `www.` stripped — **SEP-1 `DOCUMENTATION.ORG_URL`** |
| Asset **list**   | no                                                                                          | —                                                                            |

**The two sources can disagree.** They usually match, because the toml is served
from the home domain — but nothing forces an org to put its own home domain in
`ORG_URL`. Shipping the list against one source while the detail page shows the
other, both labelled "Domain", would be an inconsistency we introduced.

**The list has no choice of source.** The detail page's value comes from
fetching the issuer's `stellar.toml` during the request
(`crates/api/src/assets/handlers.rs:262-277`). Twenty list rows would mean
twenty third-party HTTP fetches per page load — not an option. The list must use
`home_domain`, which is already read per row (see below).

So decide, and record the decision here before implementing:

- **Preferred:** the list shows `home_domain`, and the detail page gains it too
  as the primary `Domain`, demoting the `ORG_URL` hostname to part of the
  `Homepage` link it came from. One meaning of "Domain" across the product: what
  the issuer declared on-chain.
- **Alternative:** keep them distinct and label them differently, which needs
  wording a user can act on — harder than it sounds, and probably not worth it.

Whichever wins, the presentation comes from the accounts-list cell — see
"Staying consistent" below for how.

## The target column does not always show an issuer

Bigger than the missing chip, and it has to be settled first. The
"Issuer / Contract ID" cell is polymorphic
(`web/src/pages/assets/AssetsTable.tsx:75-88`), and its branches are ordered:

```
contract_id      → contract  (Soroban-native asset — the contract IS the asset)
sac_contract_id  → contract  (SAC facet of a CLASSIC asset)   ← before the issuer
issuer           → issuer G…
—                → dash
```

The SAC branch wins over the issuer branch. So **for any classic asset with an
observed SAC, the column shows a `C…` contract address and never shows the
issuer at all** — and USDC, the reporter's own example, has one. The request
described `G… - Centre.io`; that row showed neither half.

Calling this a bug overstates it. The branch arrived with 0339, which collapsed
the classic↔SAC two-row split into one row — before that a SAC was its own asset
whose identity _was_ the contract, so keeping the contract visible preserved
what the old row showed. The comment written at the time justifies the _linking_
rule, not the _precedence_. It is undocumented precedence with a consequence
nobody weighed, not a mistake.

So the column has to be decided before the chip means anything:

- **Both.** The cell shows the issuer (with its domain chip) _and_ the SAC
  contract. Most informative, and it stops the SAC facet hiding the issuer — but
  it is three things in one cell and the width question below gets harder.
- **Issuer first, contract second.** Reorder so `issuer` wins for classic assets
  and the SAC contract moves to its own column or a secondary line. Matches what
  the column header promises ("Issuer / Contract ID") for classic assets.
- **Split the column.** Honest but the widest change.

Whatever wins, Soroban-native assets genuinely have no issuer and must keep
showing the contract with no chip.

## Scope

1. Add `issuer_home_domain: Option<String>` to the assets **list** item DTO
   (the detail DTO may want it too — check before assuming).
2. Regenerate API types.
3. `web/src/pages/assets/AssetsTable.tsx:69-89` — resolve the column question
   above, then render the domain beside the issuer. The StrKey stays the
   copyable canonical value.

## Staying consistent with the accounts list

The accounts list is the reference implementation
(`web/src/pages/accounts/AccountsTable.tsx:14-52`). Four details it settled that
this task must not re-litigate or silently diverge from:

- **Column width.** Accounts uses `width: 240` with the reason in a comment:
  wider than a plain identifier _because_ the cell carries the chip next to the
  address and copy button. The assets issuer column is `width: 160`, sized for
  the identifier alone — **it has to grow**. There is room: the assets table has
  only four columns (240 / 160 / 150 / 110). Match 240 rather than inventing a
  third number.
- **Extract, don't copy.** Two tables rendering "identifier + domain chip" by
  copy-paste will drift on the next change to either. Lift the accounts cell
  into a shared component and have both use it — the same move already made for
  the liquidity-pool components under `web/src/pages/pool-shared/`.
- **Same column, different tables — accept it.** Accounts reads `home_domain`
  from `accounts_recent`, the refreshable MV that is already deduped, so its
  freshness is the MV refresh interval (`crates/api/src/accounts/queries.rs:195-205`).
  Assets seek the raw `accounts` RMT with `ORDER BY last_seen_ledger DESC LIMIT 1`
  because the issuer is a _different_ row from the one being listed
  (`crates/api/src/assets/queries.rs:290-304`). Same source column, two access
  paths with different costs — do not try to unify the queries. The visible
  consequence is a divergence window after a `SET_OPTIONS` domain change, during
  which the accounts list shows the old value and the assets list the new one.
  Acceptable: a home domain changes about never.
- **Field name.** Accounts calls it `home_domain` on its own DTO
  (`crates/api/src/accounts/dto.rs:46`). On an asset the value belongs to the
  _issuer_, not the asset, so `issuer_home_domain` is the clearer name and
  matches the internal `AssetRow` field it comes from. Deliberate difference,
  not an oversight.
- **The link, verbatim.** On-chain domains carry no scheme, so accounts prefixes
  `https://` only when the stored value lacks one, and opens with
  `target="_blank" rel="noopener noreferrer"`. Keep exactly that — a bare
  `href={domain}` would resolve as a relative path.

**Out of scope:** an assets equivalent of the accounts `filter[with_domain]`
toggle. Nobody asked for it; note it here so its absence reads as a decision.

## Constraints

- **Not an identity claim.** `home_domain` is set by the account holder and is
  unverified on its own — an issuer can set any domain. Render it as a claim,
  never as a badge implying we checked it. (SEP-1 `TOML` reachability would be
  weak corroboration at best; out of scope here.)
- **Sparse.** Most issuers set no `home_domain`; the cell must look deliberate
  when the value is absent, not broken.
- The branch that renders a contract StrKey (soroban / SAC facet) has no issuer
  and must be left alone.

## Acceptance criteria

- [ ] `issuer_home_domain` present on the assets-list item response
- [ ] No additional ClickHouse round trip vs today — verified by comparing the
      query count / `read_rows` on a list page before and after
- [ ] Source decision recorded (see the two-source section) before coding
- [ ] Column decision recorded — a classic asset with a SAC shows its issuer,
      not only the SAC contract
- [ ] Verified on USDC specifically: it has a SAC, so it is the case that is
      broken today and the case the report used
- [ ] Column renders `StrKey` + domain; absent domain degrades cleanly
- [ ] Issuer column widened (160 → 240) so the chip is not crushed
- [ ] Both tables render the cell from one shared component, not two copies
- [ ] Asset detail and assets list agree on what "Domain" means — no surface
      shows a value the other contradicts
- [ ] Contract-backed rows (soroban, SAC facet) unchanged
- [ ] **Docs updated** — assets endpoint contract under `docs/architecture/**`
      per ADR 0032
- [ ] **API types regenerated** — touches `crates/api/**`; run
      `npx nx run @rumblefish/api-types:generate`

## Notes

0371 (asset search by project name / issuer domain) wants the same field as a
_search input_; this task only displays it. Landing this one first gives that
search a visible target to match against.
