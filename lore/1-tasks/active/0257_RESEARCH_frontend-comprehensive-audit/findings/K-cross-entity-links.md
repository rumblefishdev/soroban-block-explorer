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
