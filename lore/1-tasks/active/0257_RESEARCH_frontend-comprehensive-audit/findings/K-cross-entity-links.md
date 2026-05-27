# K — Cross-entity link integrity (1.7, Wave 3)

Read-only Playwright MCP sweep of all 14 routes. Goal: every clickable
entity link resolves and renders the right destination page.

Local stack snapshot at run: Vite `:4200`, API `:9000`, Mainnet selected,
seeded sample data (~20 transactions across ledgers 1015–1024). Browser
session: single tab, sequential navigation.

## Cross-entity link matrix

Rows = source page. Columns = link target type. ✓ = link present and
target renders. ✗ = link absent. STUB = target is the `PageStub`
placeholder (E3). N/A = no logical link.

| From entity ↓ / To → | tx detail | account | asset | contract | ledger | pool | nft |
|---|---|---|---|---|---|---|---|
| `/transactions` list                        | ✓ STUB | ✓ | N/A   | N/A   | ✓ | N/A | N/A |
| `/transactions/:hash` (STUB E3)             | N/A    | ✗ | ✗     | ✗     | ✗ | ✗   | ✗   |
| `/ledgers` list                             | N/A    | N/A | N/A | N/A   | ✓ | N/A | N/A |
| `/ledgers/:seq` detail                      | ✓ STUB | ✓ | N/A   | N/A   | ✓ self / prev | N/A | N/A |
| `/accounts/:id` detail — summary            | N/A    | ✓ self | N/A | N/A | ✓ first/last seen | N/A | N/A |
| `/accounts/:id` detail — recent tx table    | ✓ STUB | N/A | N/A | N/A   | ✓ | N/A | N/A |
| `/assets` list                              | N/A    | ✓ issuer | ✓ | ✓ (SAC) | N/A | N/A | N/A |
| `/assets/:id` detail                        | ✓ STUB | ✓ issuer | ✓ self | ✓ (SAC) | ✓ | N/A | N/A |
| `/contracts/:id` detail                     | N/A    | ✓ creator | N/A | ✓ self | ✓ | N/A | N/A |
| `/nfts` list                                | N/A    | ✓ owner | N/A | ✓ collection | N/A | N/A | ✓ |
| `/nfts/:id` detail                          | ✓ STUB | ✓ owner+creator | N/A | ✓ collection | N/A | N/A | N/A |
| `/liquidity-pools` list                     | N/A    | N/A | N/A | N/A   | N/A | ✓ | N/A |
| `/liquidity-pools/:id` detail               | N/A    | ✓ participants | **✗ reserves** | N/A | **✗ since-ledger** | ✓ self | N/A |

## Findings

### F-K-1 🟠 HIGH `[Class B, Severity HIGH]` — confirms Wave 1 A1: tx detail is stub

**Confirmed.** Navigated to `/transactions/7b9bacc894c4580b684692d82e03cc63d2185d3ff09ead8746736e88b2d92089`.
Page renders only:

```
Transaction/transactions/:hash
hash 7b9bacc894c4580b684692d82e03cc63d2185d3ff09ead8746736e88b2d92089
```

Source: `web/src/pages/TransactionDetailPage.tsx` → `PageStub`. Every
single tx-hash link from every list page (transactions, ledger detail,
account detail, asset detail, nft detail) lands on this stub. Cascade:
9 cells in 1.5 state matrix for E3 are N/A; 1.7 cannot complete the
"outbound from tx detail" row of the matrix (account / asset / contract
/ ledger / pool / nft links — none rendered).

Validation also missing: `/transactions/nothash` renders the stub with
`hash=nothash` and zero error. Real implementation must validate.

### F-K-2 🟠 HIGH `[Class B, Severity HIGH]` — Pool detail reserve labels not linked to asset

`/liquidity-pools/fac63b507d747965ff7fb69a48b18c4b19e1cd5b8648925246a386e4d00d87b7`
shows reserves "USDCOIN reserve 300 USDCOIN" and "EUR reserve 300 EUR"
but neither asset name is a link. Other detail pages (assets list rows,
account balances rows) link the asset name to `/assets/:id`. Inconsistent.

**Repro:** Pool detail → inspect reserve label HTML → no `<a>`.

Source: likely `web/src/pages/pool-detail/PoolSummary.tsx`.

### F-K-3 🟠 HIGH `[Class B, Severity HIGH]` — Pool participants "Since ledger" column not linked

Pool participants table cells: account = linked (✓), shares = number,
share % = number, **"Since ledger" = plain number** (e.g. `1,019` /
`1,021`) with no `<a href="/ledgers/1019">`. Same column is linked on
every other table that exposes a ledger seq. Inconsistency.

Source: `web/src/pages/pool-detail/` (participants table).

### F-K-4 🟡 MEDIUM `[Class B, Severity MEDIUM]` — Pool URL uses hex, label shows strkey

Pool list link: text = `LD5MMO...O6TL` (strkey), href =
`/liquidity-pools/fac63b507d747965ff7fb69a48b18c4b19e1cd5b8648925246a386e4d00d87b7`
(hex). User-facing identifier and routing identifier are different
encodings of the same pool. Wave 1 B1 already flagged "strkey vs hex
strategy" — this is one concrete instance. Decision needed: pick one
canonical URL form (hex is current, but strkey is the user-visible ID
and matches Stellar conventions).

### F-K-5 🟢 LOW `[Class D, Severity LOW]` — Account self-link

`/accounts/:id` detail summary renders the account ID as a link to
itself (same `/accounts/:id`). Redundant — no other detail page links
to itself. Cosmetic.

### F-K-6 🟢 LOW `[Class D, Severity LOW]` — Account TX table missing source-account column

`/accounts/:id` detail's "Recent transactions" table columns: Hash,
Ledger, Operation, Status, Fee, Time. No source-account column (because
all rows are this account by definition). Consistent. No issue. (Logged
for matrix completeness.)

## Dead links / 4xx / 5xx network calls observed

| URL clicked | Result | Network response |
|---|---|---|
| `/transactions/:hash` (any) | STUB renders (no API call) | none |
| `/transactions/nothash` (invalid id) | STUB renders | none |
| `/foobar` (unknown route) | 404 "Page not found" catch-all | none |
| `/transactions/` (trailing slash) | renders /transactions list | normal |
| `/transactions?op=invoke_host_function` (lowercase) | 0 rows, MUI warning, API 400 | `GET /v1/transactions?filter[operation_type]=invoke_host_function → 400` |

The lowercase op-filter 400 is the same case-sensitivity gap that Wave 2
C-2 catalogued. URL state preserves the bad value rather than
normalising it.

## Confirmation of Wave 1 A1 stub

Snapshot text of `/transactions/<valid hash>`:

```
Transaction/transactions/:hash
hash
7b9bacc894c4580b684692d82e03cc63d2185d3ff09ead8746736e88b2d92089
```

— exactly `PageStub` output, zero data. **A1 stands.**

## Class breakdown for K (Wave 3 1.7)

| Class | Count |
|---|---:|
| A — baseline-breaker | 0 |
| B — routing/contract | 4 (K-1, K-2, K-3, K-4) |
| C — visual/layout | 0 |
| D — catalog-only | 2 (K-5, K-6) |
| E — off-band | 0 |

## Severity breakdown

| Severity | Count |
|---|---:|
| 🔴 CRITICAL | 0 (K-1 is the cascade-class CRITICAL but inherited from Wave 1 A1) |
| 🟠 HIGH | 3 (K-1 inherited, K-2, K-3) |
| 🟡 MEDIUM | 1 (K-4) |
| 🟢 LOW | 2 (K-5, K-6) |

## Post-merge update 2026-05-25 — develop @ 6b7fb558 (FilipDz tx-detail PR #215)

**F-K-1 (🟠 HIGH — Wave 1 A1 stub confirmation):** **RESOLVED** in commit
`a2c1b205`. Real TxDetail page lives at
`web/src/pages/transaction-detail/index.tsx`; invalid hash now renders
`NotFoundState entity="transaction"` (line 36). The "outbound from tx
detail" matrix row also becomes measurable — accounts surface via
`IdentifierWithCopy type="account"` on source + signatures + flow tree
destination; contracts surface via `IdentifierWithCopy type="contract"`
in `OperationJsonDetail` and the `OperationFlowTree` invocation nodes.
**1.7 re-run scope for E3-outbound:** ~20 min Playwright pass (see
worklog Phase 4 re-audit queue).

**Validation gap (was "no validation" — A1 cascade):** **RESOLVED.**
`useTxHashParam.ts:9-12` validates via `isTransactionHash` (libs/ui
identifier validator); `/transactions/nothash` now renders
`NotFoundState` instead of stub.

**F-K-2, F-K-3, F-K-4, F-K-5, F-K-6:** STILL STAND. Filip didn't touch
pool detail or account detail areas.

**NEW FINDING — F-K-7 🟡 MEDIUM `[Class B]` — E3 tx-detail does NOT link to
ledger detail.** `TransactionSummary.tsx:148-155` renders the ledger
sequence as `<IdentifierDisplay value={String(tx.ledger_sequence)}
type="ledger" />`. Need Playwright verification whether
`IdentifierDisplay type="ledger"` renders an `<a href>` to
`/ledgers/:seq` (it should, per L1 type-defaults map) — if not, this is
a missing cross-entity link. Defer concrete verdict to delta Playwright.

**NEW FINDING — F-K-8 🟡 MEDIUM `[Class C]` — Soroban call tree destination
account routing.** `toFlowNodes.tsx:163-171` collects a
`destination_account` from heavy `invocations` recursion and renders it
as `{ kind: 'destination', identifier: { value, type: 'account' } }`.
Routing through `OperationFlowTree` (libs/ui) needs confirmation it
exposes the identifier as a clickable link — verify in delta pass.

## Post-Gate-B research finding — PoolAssetLeg schema gap

### F-K-9 🟠 HIGH `[Class B routing/contract]` — `PoolAssetLeg` lacks linkable asset identifier

**Date added:** 2026-05-25 (during 0263 correctness research, post-Gate-B)

**Trigger:** Verifying task 0263 (pool detail reserve labels Link wrap) — proposed `routes.asset(...)` call needed an asset ID. Inspection of generated types revealed PoolAssetLeg doesn't carry one.

**Evidence:**

- `libs/api-types/src/generated/types.gen.ts:1155-1166` — `PoolAssetLeg` shape:
  ```ts
  type PoolAssetLeg = {
    asset_code: string;
    asset_type: string;
    asset_type_name: string;
    issuer?: string | null;
  };
  ```
  **No `id`, no `contract_id`.**

- Asset endpoint accepted formats (per `crates/api/src/assets/handlers.rs:get_asset`):
  - numeric `assets.id`
  - contract C-strkey (56 chars)
  - `code-issuer` composite (e.g. `USDC-GA...XYZ`)

- Pool reserve leg-by-type capability to link:
  | Leg type | Has `code`? | Has `issuer`? | Linkable from PoolAssetLeg? |
  |---|---|---|---|
  | Native (XLM) | ✓ (`XLM`) | ✗ | **No** — no `id`, no `issuer`, no `contract_id` |
  | Classic credit (e.g. USDC) | ✓ | ✓ | **Partial** — `code-issuer` composite works ✓ |
  | SAC (Stellar Asset Contract) | ✓ | ✓ (maybe) | **No** — canonical link target = `contract_id` (C-strkey), not in shape |
  | Soroban contract token | ✓ | ✗ | **No** — needs `contract_id`, not in shape |

**Severity / impact:**

- Audit Wave 3 1.7 cross-entity link integrity sweep flagged the symptom (F-K-2 reserve labels plain text) but didn't dig to the schema cause.
- Backend response shape is the root blocker. FE Link wrap (F-K-2 fix) cannot ship without adding a linkable identifier to the wire format.
- Cross-task implication: every pool reserve, every pool list row's reserve display, every pool history surface that surfaces a leg-by-leg breakdown is affected.

**Class:** B routing/contract — backend wire format affects what the FE can route.

**Action:** Backend extends `PoolAssetLeg` with linkable identifier (`asset_id` numeric, `contract_id` C-strkey, or both — team decision). FE consumes via Link wrap. Folded into task 0263 scope per user 2026-05-25 (instead of spawning separate 0266) — single full-stack feature, atomic PR, one OpenAPI regen.

**Estimate:** ~30min backend (schema extend + populate in pool queries + test) + ~10min FE (Link wrap × 2) + ~5min API types regen = ~45min total (full-stack PR).

**Status:** Spawned into task 0263 (rewritten 2026-05-25). See `lore/1-tasks/backlog/0263_BUG_pool-detail-cross-entity-links-backend-and-fe.md`.

---

## Exhaustive cross-entity link sweep 2026-05-26 (pre-Wave-6)

Trigger: Wave 3 1.7 was Playwright-driven; this sweep is the code-grep
complement to confirm no additional unlinked identifiers exist.

### Linked identifier surfaces (clean — using IdentifierDisplay/IdentifierWithCopy/RouterLink)

22 files / 80 component-call sites grep-verified using
`IdentifierDisplay` or `IdentifierWithCopy`, plus dedicated `<Link
component={RouterLink}>` patterns in:
`web/src/pages/accounts/AccountBalances.tsx` (asset code → asset detail);
`web/src/pages/assets/AssetsTable.tsx` (asset code → asset detail);
`web/src/pages/nfts/NftNameCell.tsx` (NFT name → NFT detail);
`web/src/pages/home/ViewAllLink.tsx`;
`web/src/pages/ledgers/LedgerNav.tsx`;
`web/src/pages/detail/PageBreadcrumb.tsx`;
`web/src/pages/LedgerDetailPage.tsx` (ledger breadcrumb);
`web/src/pages/NftDetailPage.tsx` (NFT breadcrumb).

### Unlinked identifier renderings — NEW EXHAUSTIVE LIST

| File:line | Identifier | Type | Render mode | Link target | Severity | Finding |
|---|---|---|---|---|---|---|
| `web/src/pages/pool-detail/PoolSummary.tsx:33-34` | reserve asset code | asset | plain `Typography` in AssetReserveCell | `/assets/:id` | 🟠 | **F-K-2** (existing) |
| `web/src/pages/pool-detail/PoolKpiStrip.tsx:82-83,88-89` | reserve label + subtitle asset code | asset | plain `Typography` in KpiCell | `/assets/:id` | 🟠 | **EXTENDS F-K-2** — additional pool detail surface |
| `web/src/pages/liquidity-pools/PoolsTable.tsx:97-105` | reserve column asset code (list page) | asset | plain `Typography` in reserves stack | `/assets/:id` | 🟠 | **EXTENDS F-K-2** — list-page surface |
| `web/src/pages/pool-detail/PoolParticipants.tsx:57-59` | `first_deposit_ledger` "Since ledger" | ledger | plain `Typography` w/ `formatAmount` | `/ledgers/:seq` | 🟠 | **F-K-3** (existing) |
| `web/src/pages/nft-detail/NftSummary.tsx:87-89` | `minted_at_ledger` | ledger | plain `Typography` w/ inline comment "Plain Satoshi text per Figma" | `/ledgers/:seq` | 🟡 | **NEW — F-EX-1** |
| `web/src/pages/contracts/ContractEvents.tsx:78-90` | event topic strings (when string-typed) | unknown (could be account/contract) | plain colored `Typography` w/ `shortStr` 4/4 truncate | unclear — topics are unstructured | 🟢 | informational; topics may carry addresses |
| `web/src/pages/contracts/ContractEvents.tsx:96-126` | event data cell | freeform JSON | plain `Typography` middle-truncated | N/A | N/A | not an identifier |

### Implication for task 0263

Original task 0263 scope: pool detail reserve labels (PoolSummary) +
PoolAssetLeg backend schema extend.

**Exhaustive surface (3 sites, same backend schema root cause F-K-9):**

1. `web/src/pages/pool-detail/PoolSummary.tsx` (2 reserve cells)
2. `web/src/pages/pool-detail/PoolKpiStrip.tsx` (2 reserve KPI cells) — **NEW**
3. `web/src/pages/liquidity-pools/PoolsTable.tsx` (list page reserves column) — **NEW**

All 3 unblocked by same backend `PoolAssetLeg` extension. Recommend user
extend task 0263 to confirm FE Link wrap applies to all 3.

### Implication for task 0264 — none

No additional endpoints with strkey/hex form drift discovered. Pool is
the only case.

### Other observation — F-EX-1 (NFT minted_at_ledger plain text)

The `NftSummary.tsx:88` comment explicitly says "Plain Satoshi text per
Figma — not a mono/linked identifier." Tension with every other ledger-
sequence in the UI being linked. May be deliberate Figma intent or
oversight. Defer to Gate B visual audit.

See also `findings/exhaustive-sweep-2026-05-26.md` for full sweep details.

## Gate B merge resolution 2026-05-26 — develop @ cdb0c81d (PR #219)

### F-K-2 — **RESOLVED** in `473de2a2` + `a5f15166`

Pool detail reserve labels wrapped in router `<Link to={routes.asset(...)}>` across **3 sites** (post-sweep scope correction from initial 1 site):

- `web/src/pages/pool-detail/PoolSummary.tsx` (AssetReserveCell) — via `legHref` precedence
- `web/src/pages/pool-detail/PoolKpiStrip.tsx` — per-leg KPI subtitle (NEW per sweep — `a5f15166`)
- `web/src/pages/liquidity-pools/PoolsTable.tsx` — reserve column asset codes on list page (NEW per sweep — `a5f15166`)

Unblocked by backend `PoolAssetLeg` schema extension (see F-K-9 below).

### F-K-3 — **RESOLVED** in `473de2a2`

`PoolParticipants.tsx` "Since ledger" column wrapped in `<Link to={routes.ledger(seq)}>`. Per task 0263 acceptance criterion `[x] PoolParticipants.tsx wraps Since-ledger cell in RouterLink`.

### F-K-9 — **RESOLVED** in `473de2a2`

`PoolAssetLeg` backend schema extended with linkable identifier; `crates/api/src/liquidity_pools/queries.rs` populates new field; OpenAPI regen committed (`libs/api-types/src/openapi.json` + `libs/api-types/src/generated/types.gen.ts`); FE consumes via the 3-site Link wrap above. Full-stack atomic landing per merged 0263 task body.

### F-K-4 — **STILL OPEN** (search portion deferred)

Empty-state hint `L...` addition deferred to `future-search-followup` follow-up task per 0264 Gate B mid-PR scope correction (search-related Phases 3 + 9 + 10 + Fala 3 reverted; full search work spawned separately). See 0264 archive task body §Issues for deferral rationale.

### F-K-5 + F-K-6 + F-K-7 + F-K-8 — **STATUS UNCHANGED**

Account self-link (K-5), missing source-account column for accounts list (K-6), E3 tx-detail ledger link verification (K-7), Soroban call tree destination account routing (K-8) — none touched by Gate B batch. Will surface in Wave 6 Track 2 Playwright re-pass if remaining issues.

## 0270 merge resolution 2026-05-27 — develop @ cb2fa80a (PR #220)

### F-K-4 — **RESOLVED** in `6421d3d7`

`SearchResultsView` empty-state hint extended to include liquidity pool `L…` prefix alongside `G…` (account) and `C…` (contract). User pasting strkey from stellar.expert now sees pool in supported formats list. Paired with F-L-1 backend resolution.

### Bonus: NFT search-404 regression (carry-over from 0264 Phase 8a) — **RESOLVED** in `6421d3d7` + `69d9f529`

Background: 0264 Phase 8a refactored `/v1/nfts/:id` → `/v1/nfts/:contract_id/:token_id`. `routeForHit` was at HEAD shape emitting `/nfts/<surrogate>` → React Router couldn't match composite → hard 404. Soft-fallback in `9c3db048` reverted in `4716d5f3` per user decision to defer proper composite fix to 0270.

Fix: `SearchHit` extended with optional `contract_id` + `token_id`; `nft_hits` CTE JOINs `soroban_contracts` to project both; FE `routeForHit` NFT-composite short-circuit calls `routes.nft(c, t)`. NFT search results now navigate to the composite path correctly.
