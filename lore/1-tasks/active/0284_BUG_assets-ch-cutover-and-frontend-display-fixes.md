---
id: '0284'
title: 'Assets CH-cutover orphan + contract-type label parity + identifier/LP display fixes'
type: BUG
status: active
related_adr: []
related_tasks: ['0243', '0257', '0283']
tags:
  ['effort-small', 'priority-high', 'area-api', 'area-frontend', 'area-infra']
links: []
history:
  - date: 2026-06-11
    status: active
    who: karol
    note: 'Task created — spawned mid-session to carry a cluster of small bug/display fixes found while triaging "assets list does not load" on dev.'
---

# Assets CH-cutover orphan + contract-type label parity + identifier/LP display fixes

## Summary

A cluster of small, independent fixes found while triaging "assets list +
detail do not load on dev". Root cause of the headline bug: after the
PG→ClickHouse cutover (task 0243), the `assets` module was left on the PG
default while every working endpoint reads from CH — and PG is no longer
the live store, so assets served nothing. Bundled alongside: a CH-vs-PG
contract-type label parity bug, plus three frontend display fixes
(identifier truncation, clickable home domains, LP amount formatting).

## Context

Found during a dev triage session (not the active 0283 task). Each item is
small and independently shippable; grouped here so the branch + commits have
a home.

## Implementation Plan / Items

1. **Assets module orphaned on PG (headline bug).**
   `infra/src/lib/stacks/compute-stack.ts` enabled CH for 6 of 9 modules
   (network, ledgers, transactions, accounts, contracts, liquidity_pools)
   but NOT `assets` (nor nfts/search). `DataSource::for_module` defaults to
   `Pg` when the env is unset; PG is no longer the live store post-cutover,
   so `/assets` list + detail (shared PG path) served nothing. The CH path
   (`crates/api/src/assets/queries_ch.rs`) already exists, mirroring the
   contracts/accounts modules. **Fix:** set `API_DATASOURCE_ASSETS: 'ch'`.
   Requires deploy + operator CH read-rows smoke before prod reliance.
   nfts/search left on PG deliberately (out of scope here).

2. **CH `contract_type_name` label parity.**
   `crates/api/src/contracts/queries_ch.rs` mapped only `0→token, 1→other`;
   `nft(2)`/`fungible(3)` returned `None`, so on the CH datasource those
   rows rendered "Unknown" instead of the canonical label the filter chips
   offer. PG's SQL `contract_type_name` (migration
   `20260422000100_contract_type_add_nft_fungible`) already covers all four;
   canonical source is `domain::ContractType`. **Fix:** extend the Rust fn +
   its unit test to cover nft/fungible.

3. **Identifier truncation + copy in detail/list views.**
   Account detail under-title and balances issuer subline, and contract
   detail under-title, rendered full identifiers as plain text instead of
   the `IdentifierDisplay`/`IdentifierWithCopy` component (full id stays in
   the summary card). Contracts list contract-id gained a copy button.

4. **Clickable home domains.**
   Accounts list rendered the issuer `home_domain` as a passive Chip;
   wrapped in an external link (`https://<domain>`, `_blank`).

5. **LP amount formatting.**
   Pool reserves + total shares rendered with full 7-decimal stroop
   precision (`7,260.4903523 NGNT`) — comma (thousands) and dot (decimal)
   read alike at a glance. Switched to the existing `formatCompactAmount`
   (`7.26K`); `participant_count` (an integer count) stays on `formatAmount`.

## Acceptance Criteria

- [x] `API_DATASOURCE_ASSETS: 'ch'` set in compute-stack.ts (deploy + CH
      smoke tracked separately).
- [x] CH `contract_type_name` covers nft/fungible; unit test updated +
      passing.
- [x] Account/contract detail under-title + balances subline use the
      identifier component (truncated); contracts-list id has a copy button.
- [x] Accounts-list home domains are clickable external links.
- [x] LP reserves + total shares use compact formatting.
- [x] `nx typecheck` (web + ui) green; `cargo check -p api` green.
- [ ] **Docs updated** — N/A. No system-shape change: a datasource flag
      flip (0243 mechanism, already documented), a CH-vs-PG value-mapping
      parity fix, and frontend presentation tweaks. No schema/endpoint/
      ingestion/topology change.
- [ ] **API types regenerated** — N/A. `queries_ch.rs` change is a runtime
      value mapping, not an OpenAPI schema change; `infra/**` is not a
      codegen trigger path. No `crates/api` DTO/schema or `Cargo.{toml,lock}`
      change.

## Notes

- nfts + search remain on the PG default (same orphan class as assets) —
  their pages will still be empty on dev. Out of scope here; flag if needed.
- Assets type filter (`ASSET_TYPE_FILTERS`) lacks a "Native" option though
  rows can show a "Native" chip — minor analogous inconsistency, not fixed.
- Live-polling feature work present in the same worktree is SEPARATE
  (pre-existing) and is NOT part of this task.
