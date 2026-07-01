---
id: '0051'
title: 'SAC is a facet of classic_credit, not a separate asset_type'
status: proposed
deciders: [stkrolikiewicz]
related_tasks: ['0339', '0323', '0210', '0331']
related_adrs: ['0034', '0037', '0038']
tags: [clickhouse, sac, assets, schema, api, frontend, contract-classification]
links:
  - 'https://developers.stellar.org/docs/tokens/stellar-asset-contract'
  - 'https://developers.stellar.org/docs/tokens/anatomy-of-an-asset'
history:
  - date: '2026-06-30'
    status: proposed
    who: stkrolikiewicz
    note: >
      ADR created for task 0339 (collapse the classic↔SAC entity split). Decisions
      confirmed in the SAC/asset modeling session: drop asset_type=sac, SAC-ness as
      self-contained property columns on the classic/native asset row, no CH rebuild.
---

# ADR 0051: SAC is a facet of classic_credit, not a separate asset_type

**Related:**

- [Task 0339: collapse the classic↔SAC entity split](../1-tasks/active/0339_REFACTOR_sac-as-facet-of-classic-not-separate-type.md)

---

## Context

A classic asset (`CODE:ISSUER`) and its Stellar Asset Contract (SAC) are the **same
economic asset** — the Stellar docs call them "the same asset", the SAC being "an API
for interacting with the asset" (it debits/credits the same trustlines; an un-deployed
SAC is "merely a reserved address, neither an asset nor an active contract").

Our `assets` table splits this one asset into **two rows**: `asset_type=1`
(classic*credit, by `code:issuer`) and `asset_type=2` (sac, carrying the `contract_id`).
This is a conflation of \_identity* with _addressing/interface_, and an evolutionary
artifact — the system was Soroban-first (0154), with `classic_credit` written added
later (0219), so `sac` predates `classic_credit`.

Symptoms the split causes (currently band-aided):

- **Duplication** on `/v1/assets` + a non-deterministic by-code-issuer resolver → 0336.
- A **misleading `contract_id`** that links to a non-existent contract page for
  un-deployed SACs → 0337.

The split is reinforced at TWO layers (audit, 2026-06-30):

- **CH key** — `assets ORDER BY (asset_type, asset_code, issuer_id, contract_id)`
  (`init.sql:226`); `asset_type` in the key keeps classic and SAC as distinct rows.
- **Staging fingerprint** — `staging.rs:996-1027` dedups `sac|{contract_id}` separately
  from `classic_credit|{code}|{issuer}`, fed by two emission paths
  (`detect_classic_credit_assets` + `detect_assets` SAC branch).

`asset_aggregates` already co-aggregates types 1+2 under one `(asset_code, issuer_id)`
row (`init.sql:290-300`) — i.e. the system already treats them as one asset economically,
but stores/lists/links them as two.

---

## Decision

SAC-ness becomes a **property** of a `classic_credit` (or `native`) asset, not a separate
`asset_type`. Concretely:

1. **Drop `asset_type = sac (2)`.** Enum → `{native=0, classic_credit=1, soroban=3}`.
   `soroban` (type=3, bespoke, no classic backing) remains the only genuinely
   contract-native asset type.

2. **Add 3 SAC property columns to `assets`** so the asset row is self-contained
   (no `soroban_contracts` join on the asset read):

   - `sac_contract_id Int64 DEFAULT 0` — the SAC surrogate (for resolution/indexing).
   - `sac_strkey String DEFAULT ''` — the `C…` text, re-derivable from `code:issuer`
     (subsumes the deferred "option-c" strkey display).
   - `sac_deployed Bool DEFAULT false` — deployed-ness, writer-maintained.

3. **Keep the `assets` ORDER BY unchanged** — `(asset_type, asset_code, issuer_id,
contract_id)`. **No table rebuild / no ORDER-BY change.** A SAC-wrap is written as
   `type=1` (classic) or `type=0` (native) with key `contract_id=0` and the SAC handle in
   the property columns. The key `contract_id` stays in use **only for `soroban` (type=3)**.
   Result: one row per economic asset (classic+SAC collapse on `(code,issuer)`; native +
   XLM-SAC collapse on the native singleton; soroban keyed by `contract_id`).

4. **Converge the write path.** A SAC deploy/override UPDATES the classic/native asset's
   SAC columns instead of emitting a separate `type=2` row; the staging fingerprint folds
   SAC into the `classic_credit`/`native` fingerprint.

5. **API.** Drop `sac` from `asset_type_name` / `filter[type]`; surface
   `sac_contract_id` / `sac_strkey` / `sac_deployed`; the "SAC" view becomes a property
   filter (`classic_credit WHERE sac_contract_id != 0`). Canonical `id` for SAC-wraps
   becomes `CODE-ISSUER`; `fetch_by_contract_id` is extended to also match
   `sac_contract_id` / `sac_strkey` so `/assets/{C…}` deep-links still resolve.

6. **Frontend.** "SAC" filter → property filter; SAC badge derived from
   `sac_contract_id != 0`; the contract link renders from `sac_strkey` with
   deployment-awareness (`sac_deployed`) — **subsumes 0337**.

---

## Rationale

- **Matches the asset's reality** (Stellar docs: one asset; SAC = an API facet) and the
  standard explorer model (Horizon: classic asset is the asset; SAC is a
  contract-with-deployment-status, not a separate asset).
- **No CH rebuild.** Keeping `asset_type` in the key and simply not using value 2 (with the
  handle moved to property columns) gives one-row-per-economic-asset without an ORDER-BY
  change or new-table swap — the migration reduces to `ADD COLUMN` + a ~31k-row data pass.
- **Shortest read.** Storing `sac_strkey` + `sac_deployed` makes the asset row
  self-contained → the asset read needs **no `soroban_contracts` join**, and it folds in
  the option-c strkey display in one move.
- **Subsumes the band-aids.** One model fix replaces 0336 (read-collapse) + 0337 (link
  guard) + option-c, at the source rather than as read-time / UI patches.

---

## Alternatives Considered

### Alternative 1: Keep two types, band-aid the symptoms (0336 read-collapse + 0337 guard)

**Description:** Leave the split; dedup on read (0336) and guard the link in the UI (0337).

**Cons:** treats symptoms, not the root; duplication + non-deterministic resolver persist
in storage; permanent read-collapse overhead; the misleading-link guard is a UI patch over
a model error.

**Decision:** REJECTED — 0336 and 0337 are superseded by this ADR.

### Alternative 2: Change the `assets` ORDER BY / rebuild the table

**Description:** New sort key keyed by economic identity; create new table, backfill, swap.

**Cons:** unnecessary and risky. CH cannot change ORDER BY in place; the non-partitioned
`assets` table would need new-table + backfill + coordinated swap.

**Decision:** REJECTED — keeping `asset_type` in the key and not using value 2 achieves the
same one-row result with only `ADD COLUMN` + a data pass.

### Alternative 3: Derive `deployed` from a `soroban_contracts` join (no stored flag/strkey)

**Description:** No SAC property columns beyond the surrogate; read joins
`soroban_contracts` for the strkey + `deployed_at_ledger`.

**Cons:** keeps the asset read coupled to `soroban_contracts`; does not fix the strkey
display (option-c stays open).

**Decision:** REJECTED — storing `sac_strkey` + `sac_deployed` yields a self-contained,
no-join read and subsumes option-c. Drift risk is low (the writer sets the deploy row and
the SAC columns together).

---

## Consequences

### Positive

- One `assets` row per economic asset — no classic↔SAC duplication; deterministic resolver.
- Self-contained asset read (no `soroban_contracts` join) — shortest query, decoupled.
- Subsumes 0336 (duplication), 0337 (link guard), and option-c (strkey display) in one model.
- Aligns with the standard Stellar / Horizon model.
- No CH table rebuild — migration is `ADD COLUMN` + a ~31k-row data pass.

### Negative

- Migration data-pass (type=2 → merge into type=1/0), **writer-first** (deploy the writer
  change → then the pass, else rows regrow) — coordinated like 0323 Phase 1/2.
- Writer maintains 3 SAC columns (`sac_contract_id` / `sac_strkey` / `sac_deployed`), set
  together at deploy/derivation.
- Canonical-id wire change: SAC-wrap id `C… → CODE-ISSUER`; `/assets/{C…}` deep-links
  handled by the extended resolver (back-compat preserved).
- Enum / DTO / frontend-filter ripple from dropping `sac=2`; api-types regen.

---

## Delivery Checklist

Docs land WITH the implementation (task 0339); this ADR is `proposed`.

- [ ] `docs/architecture/technical-design-general-overview.md` — N/A (no top-level shape change).
- [ ] `docs/architecture/database-schema/database-schema-overview.md` — **on implementation** (assets columns + asset_type taxonomy).
- [ ] `docs/architecture/backend/backend-overview.md` — **on implementation** (asset DTO / resolvers / filter).
- [ ] `docs/architecture/frontend/frontend-overview.md` — **on implementation** (SAC filter → property; link rendering).
- [ ] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` — N/A (asset-shape, not pipeline-topology).
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md` — N/A.
- [ ] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` — **on implementation** (write-path convergence: SAC folds into classic/native row).
- [ ] This ADR linked from each updated doc — on implementation.

---

## References

- [Stellar Asset Contract | Stellar Docs](https://developers.stellar.org/docs/tokens/stellar-asset-contract) — "the same asset" / "an API for interacting with the asset".
- [Anatomy of an Asset | Stellar Docs](https://developers.stellar.org/docs/tokens/anatomy-of-an-asset).
- Audit (2026-06-30): current-state map of schema/write-path/API/frontend + migration mechanics (task 0339 session).
