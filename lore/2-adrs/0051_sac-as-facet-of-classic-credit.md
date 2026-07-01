---
id: '0051'
title: 'SAC is a facet of classic_credit, not a separate asset_type'
status: accepted
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
  - date: '2026-06-30'
    status: accepted
    who: stkrolikiewicz
    note: >
      Accepted. Refined in review: 2 stored SAC columns (`sac_contract_id` surrogate +
      `sac_deployed` Bool), NOT 3 — the `C…` strkey is re-derived on read from `code:issuer`,
      not stored (Alt 3). Clarified why the SAC handle cannot reuse the key `contract_id`
      (it is the ORDER-BY identity, reserved for soroban; reusing it regrows the duplication
      on deploy — Alt 4). Docs land with the 0339 implementation.

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

2. **Add 2 SAC property columns to `assets`** (non-key — the asset's identity / ORDER-BY
   tuple stays stable regardless of SAC deploy state → one row):

   - `sac_contract_id Int64 DEFAULT 0` — the SAC surrogate. **Stored + indexed**: it is the
     lookup key that resolves Soroban activity (events/tx under `C…`) back to the asset, and
     it cannot be derived-and-matched efficiently. Populated for ANY SAC-having asset —
     deployed or not (un-deployed SACs still emit events that must resolve). Kept OUT of the
     ORDER-BY key on purpose; the key `contract_id` is reserved for `soroban` (type=3),
     where the contract IS the identity (see §3).
   - `sac_deployed Bool DEFAULT false` — deployed-ness, writer-maintained. **Stored** (not
     derived from a `soroban_contracts` join) so the asset read stays join-free.

   The `C…` **strkey is NOT stored** — it is a pure function of `code:issuer`
   (`derive_sac_contract_id`), so it is **re-derived on read** (API response layer) for
   display. This keeps the read join-free without denormalising a derivable value and still
   subsumes the deferred "option-c" strkey display. (`sac_contract_id` is stored anyway
   because it is the resolution index — a `C…` lookup hashes the input to this surrogate.)

3. **Keep the `assets` ORDER BY unchanged** — `(asset_type, asset_code, issuer_id,
contract_id)`. **No table rebuild / no ORDER-BY change.** A SAC-wrap is written as
   `type=1` (classic) or `type=0` (native) with key `contract_id=0` and the SAC handle in
   the property columns. The key `contract_id` stays in use **only for `soroban` (type=3)**.
   Result: one row per economic asset (classic+SAC collapse on `(code,issuer)`; native +
   XLM-SAC collapse on the native singleton; soroban keyed by `contract_id`).

4. **Converge the write path.** A SAC deploy/override UPDATES the classic/native asset's
   SAC columns instead of emitting a separate `type=2` row; the staging fingerprint folds
   SAC into the `classic_credit`/`native` fingerprint.

5. **API.** Drop `sac` from `asset_type_name` / `filter[type]`; surface `sac_contract_id`
   + `sac_deployed` + the re-derived `C…` strkey; the "SAC" view becomes a property filter
   (`classic_credit WHERE sac_contract_id != 0`). Canonical `id` for SAC-wraps becomes
   `CODE-ISSUER`; `fetch_by_contract_id` is extended to hash an input `C…` to its surrogate
   and match `sac_contract_id` so `/assets/{C…}` deep-links still resolve.

6. **Frontend.** "SAC" filter → property filter; SAC badge derived from
   `sac_contract_id != 0`; the contract link renders from the (re-derived) `C…` strkey with
   deployment-awareness (`sac_deployed`) — **subsumes 0337**.

---

## Rationale

- **Matches the asset's reality** (Stellar docs: one asset; SAC = an API facet) and the
  standard explorer model (Horizon: classic asset is the asset; SAC is a
  contract-with-deployment-status, not a separate asset).
- **No CH rebuild.** Keeping `asset_type` in the key and simply not using value 2 (with the
  handle moved to property columns) gives one-row-per-economic-asset without an ORDER-BY
  change or new-table swap — the migration reduces to `ADD COLUMN` + a ~31k-row data pass.
- **Shortest read.** `sac_contract_id` + `sac_deployed` on the row (and the `C…` strkey
  re-derived on read from `code:issuer`) make the asset read **self-contained — no
  `soroban_contracts` join** — while avoiding denormalising the derivable strkey. Folds in
  the option-c strkey display.
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

### Alternative 3: Derive `deployed` from a `soroban_contracts` join

**Description:** No stored `sac_deployed`; the asset read joins `soroban_contracts` for
`deployed_at_ledger`.

**Cons:** keeps the asset read coupled to `soroban_contracts`.

**Decision:** REJECTED for `deployed` — store `sac_deployed` (Bool) so the read stays
join-free. The strkey is the OPPOSITE call: it is NOT stored (it is derivable from
`code:issuer`, re-derived on read) — storing it would denormalise a derivable value for no
join-avoidance gain (the read is already join-free via the stored surrogate + flag).

### Alternative 4: Reuse the key `contract_id` for the SAC handle (populate only when deployed)

**Description:** No `sac_contract_id`; put the SAC contract_id in the existing key
`contract_id`, set only when the SAC is deployed.

**Cons:** breaks the one-row goal. `contract_id` is part of the ORDER-BY key (= row identity);
setting it on deploy changes a classic asset's key `(1,code,issuer,0) → (1,code,issuer,C…)`
→ a NEW row alongside the trustline row → the duplication regrows. And an un-deployed SAC
(`contract_id` absent) could not resolve its Soroban events (no stored surrogate to match).

**Decision:** REJECTED — the SAC handle must be a NON-KEY, always-populated, stored+indexed
column (`sac_contract_id`); the key `contract_id` stays reserved for `soroban` identity.

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
- Writer maintains 2 SAC columns (`sac_contract_id` + `sac_deployed`), set at
  deploy/derivation; the `C…` strkey is re-derived on read (not stored).
- Canonical-id wire change: SAC-wrap id `C… → CODE-ISSUER`; `/assets/{C…}` deep-links
  handled by the extended resolver (back-compat preserved).
- Enum / DTO / frontend-filter ripple from dropping `sac=2`; api-types regen.

---

## Delivery Checklist

The decision is `accepted`; the `docs/architecture/**` updates land WITH the implementation
(task 0339) and are ticked when 0339 lands.

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
