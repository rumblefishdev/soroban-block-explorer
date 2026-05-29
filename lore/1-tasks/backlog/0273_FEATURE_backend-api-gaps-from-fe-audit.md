---
id: '0273'
title: 'Backend: API endpoints + fields surfaced by FE gaps audit'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0199', '0226']
tags:
  [
    priority-medium,
    effort-medium,
    layer-api,
    layer-backend,
    phase-pre-launch,
    milestone-2,
  ]
milestone: 2
links:
  - docs/audits/2026-05-29-frontend-api-gaps.md
history:
  - date: '2026-05-29'
    status: backlog
    who: FilipDz
    note: >
      Spawned from the FE→API gaps audit
      (`docs/audits/2026-05-29-frontend-api-gaps.md`). FE built the
      Accounts page against an in-memory mock + worked around several
      missing fields on existing endpoints. This task tracks the
      backend work to close those gaps.
---

# Backend: API endpoints + fields surfaced by FE gaps audit

## Summary

Implement the backend pieces FE called out in the
[FE→API gaps audit](../../../docs/audits/2026-05-29-frontend-api-gaps.md):
one new list endpoint, one expanded list, and four schema /
contract additions. Closes the FE workarounds (in-memory account
mocks, hidden Amount column on pool tx, hand-rolled
`interface_metadata` parser, etc.).

## Context

The FE is in design-parity / pre-launch state. The audit catalogues
six concrete gaps blocking either a feature surface or a quality
detail:

1. **No `GET /v1/accounts` list endpoint** — Accounts page renders
   from 80 in-memory synthesized rows (`useAccountsList.ts`).
2. **Per-op LP amounts missing** on
   `GET /v1/liquidity-pools/{pool_id}/transactions` — the "Amount"
   column in the pool-tx table is intentionally hidden.
3. **`order` query param undocumented** on `GET /v1/ledgers` —
   accepted by the backend, missing from the OpenAPI spec.
4. **Pool chart values always `null`** — endpoint contract is in
   the spec but `tvl` / `volume` / `fee_revenue` are `null` for
   every bucket until task **0199** (LP analytics + price oracle)
   ships. FE renders a placeholder card.
5. **`PoolAssetLeg.icon_url` missing** — pool avatars fall back to
   the first letter of the asset code instead of a real icon.
6. **`interface_metadata` typed as `unknown`** on
   `GET /v1/contracts/{contract_id}/interface` — FE hand-parses the
   Soroban spec at runtime in
   [`interfaceMetadata.ts`](../../../web/src/pages/contracts/interfaceMetadata.ts).
7. **No real events count** on `ContractStats` — the Events tab
   pill borrows `recent_unique_callers`, which is misleading.

The audit doc has the full TypeScript shapes FE expects.

## Implementation

Each gap is independent and can ship piecemeal:

- **`GET /v1/accounts` (list)** — Query: `limit`, `cursor`,
  `sort=xlm_desc|last_seen_desc|first_seen_desc`, `filter[q]`,
  `filter[with_domain]`. Response item:
  `{ account_id, xlm_balance, xlm_supply_percent, first_seen_ledger,
last_seen_ledger, home_domain, rank? }`. Each sort mode wants its
  own DB index. Consider returning `rank` per row so FE doesn't have
  to parse the opaque cursor for the `#` column.
- **`?expand=lp_op_details`** on pool transactions — opt-in field
  per row: `lp_operation_detail { operation_type, amount_a,
amount_b }`. Tracked separately as the **0247 / 0249** envelope;
  cross-link when those move.
- **Document `order` on `/v1/ledgers`** — add the param to the
  OpenAPI schema, or remove the FE usage and document why.
- **`icon_url` on `PoolAssetLeg`** — mirror the `AssetItem` field.
- **`interface_metadata` schema** — codify the
  `{ functions[], wasm_byte_len }` JSON Schema in OpenAPI so typegen
  produces a real type and the FE defensive parser can be deleted.
- **Events count metric on `ContractStats`** — add `recent_events`
  (or similar) so the Events tab pill stops borrowing the callers
  metric.

Pool chart fields (gap #4) are covered by **0199** — link in
`related_tasks` rather than duplicate work here.

## Acceptance Criteria

- [ ] `GET /v1/accounts` ships behind the documented contract;
      FE swaps `useAccountsList` from in-memory mock to the generated
      SDK hook.
- [ ] `?expand=lp_op_details` on pool transactions is wired and the
      "Amount" column on the LP tx table is un-hidden FE-side
      (or tracked via 0247/0249).
- [ ] OpenAPI declares the `order` param on `/v1/ledgers`.
- [ ] `PoolAssetLeg` carries `icon_url`; pool avatars render real
      icons when available.
- [ ] `InterfaceResponse.interface_metadata` has a real schema in
      OpenAPI; FE deletes `parseInterfaceMetadata`'s defensive parse.
- [ ] `ContractStats` exposes a real events count; FE points the
      Events tab pill at the new field.

## Notes

- The audit doc is the single source of truth for FE-side expected
  shapes — keep it in sync if backend semantics diverge during
  implementation.
- Mock-server divergences (transactions missing `contract_ids`,
  NFTs ignoring `filter[name]`, search shape mismatch) are
  intentionally **out of scope** here — they're FE dev-mock bugs,
  not real-API blockers.
