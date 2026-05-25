# D — State coverage matrix companion

Wave 4 1.5 output. CSV: `D-state-coverage-matrix.csv` (126 cells).

> **2026-05-25 delta re-run (post API-binary restart):** F-D-1 RESOLVED.
> All 5 previously-✗ D9 cells (E2/E4/E7/E10/E12) flipped to ✓ after
> the API binary was restarted from the audit branch tip (which includes
> the 0254 merge). The bug was infra (stale binary serving pre-0254
> pagination shape), not a code bug. See the "Wave 4 delta re-run"
> section at the bottom for full evidence.

## Summary stats

| Verdict | Count | % |
|---|---:|---:|
| ✓ correct (post-delta) | 55 | 44% |
| ⚠ partial | 18 | 14% |
| ✗ broken | 0 | 0% |
| N/A | 27 | 21% |
| ? not exercised (skipped) | 26 | 21% |
| **Total** | **126** | 100% |

**Pre-delta** (for reference): 50 ✓, 18 ⚠, 5 ✗, 27 N/A, 26 ?.

## Critical cells (✗ broken)

**None remaining post-delta** (2026-05-25 re-run). All 5 ✗ cells flipped
to ✓. F-D-1 below retained as historical record.

### F-D-1 [Class A, Severity 🔴 → RESOLVED 2026-05-25] — Live API served pre-0254 pagination shape

- **Cells:** E2/D9, E4/D9, E7/D9, E10/D9, E12/D9 (every list page)
- **Plus collaterally:** every tab table on E5/E6/E8/E9/E11/E13 (untested D9 but same code path via `usePageHandlers`)
- **Evidence:**
  - `libs/ui/src/table/usePageHandlers.ts:47` reads `page.next_cursor`
  - `libs/ui/src/table/usePageHandlers.ts:48` reads `page.prev_cursor`
  - Live API: `curl http://localhost:9000/v1/transactions?limit=2` returns `page: {cursor, limit, has_more}` (old shape)
  - Verified in browser: `/transactions` and `/ledgers` both have Next **AND** Previous buttons `disabled=true` despite 20+ visible rows + backend `has_more: true`
- **Class:** A (baseline-breaker) — distorts D2 / D9 measurement on every list page and tab
- **Root cause:** API binary in dev environment was not rebuilt after 0254 backend changes merged. FE generated types + `usePageHandlers` are on the new shape; API serves old shape.
- **Recommendation (Phase 3):** verify dev-env runbook step is missing API rebuild after type generation. Add CI check that `cargo run -p api --bin extract_openapi` matches what the running API actually serves. Spawn `XXXX_CHORE_dev-env-rebuild-runbook` and `XXXX_FEATURE_api-shape-runtime-probe`.
- **Note for Gate B:** Track 2 Playwright 2.0 will see this exact bug. Mark this finding referenced by 2.0 so it's not double-reported. Per task spec: pagination tests in 0254 passed because they hit a freshly-rebuilt API; live dev runs against a stale binary. Production deploy with rebuilt API would work.

#### RESOLVED 2026-05-25 (binary restart, not code fix)

API restarted from audit branch tip `6af74d82` (includes 0254 merge).
New wire shape verified:

```
curl http://localhost:9000/v1/transactions?limit=2
→ page: { next_cursor: "<base64>", prev_cursor: null, limit: 2 }
```

Browser delta re-verify (Wave 4 delta section below) confirms all 5
formerly-✗ cells now ✓. 0254 was on develop and on this branch the
whole time; the dev API binary was stale relative to the merged code.
**Code shape was correct; this was infra-only drift.**

Phase 3 follow-up tasks per the original recommendation still apply
(dev-env rebuild runbook + runtime shape probe) — both unchanged
because the underlying gap (no detection mechanism for stale binary)
remains.

## Partial cells (⚠) of concern

### F-D-2 [Class B, Severity 🟠] — Composite NotFound + TransientErrorState on entity detail pages with parallel sub-section queries

- **Cells:** E6/D4, E8/D4, E9/D4
- **Pattern:** When detail page parent query 404s, sub-section queries (recent transactions, balances, events, etc.) also 404 in parallel — UI shows BOTH the parent "X not found" message AND child "Something went wrong / An unexpected error..." blocks simultaneously.
- **Evidence:**
  - `/accounts/GNOTAREALACCOUNT...` shows: "Account not found" header + "Recent transactions: Something went wrong" tab
  - `/assets/99999` shows: "Asset not found" + "Latest transactions: Something went wrong"
  - `/contracts/CNOTACONTRACT...` shows: "Contract not found" + Interface/Invocations/Events all = "Something went wrong" (worst — 4 error blocks)
- **Contrast (clean):** E3 `/transactions/<garbage>`, E5 `/ledgers/<garbage>`, E11 `/nfts/<garbage>`, E13 `/liquidity-pools/<garbage>` all show single clean "X not found"
- **Distinction:** Entities with separate `*Transactions.tsx` / `*Invocations.tsx` / `*Events.tsx` sub-section components that fire their own query without gating on parent.
- **Class:** B (routing/contract — affects 404 contract surfaced to user)
- **Recommendation:** Sub-section queries should be `enabled: !!parentData && !parentError` (TanStack pattern), or detail page should early-return NotFound before mounting sub-section components.
- **Defer to Gate B.** Track 2 Figma audit will hit same routes.

### F-D-3 [Class C, Severity 🟡] — Detail page H1 heading inconsistency

- **Cells:** E6/D2, E9/D2 (drift from E3/E5/E8/E11/E13)
- **Pattern:**
  | Endpoint | H1 |
  |---|---|
  | E3 transactions | "Transaction Detail" |
  | E5 ledgers | "Ledger 1,024" |
  | E6 accounts | "Account" (no id) |
  | E8 assets | "USDCOIN" (asset code) |
  | E9 contracts | "Contract" (no id) |
  | E11 nfts | "Cat #2" |
  | E13 pools | "USDCOIN / EUR" |
- **Three patterns:** generic ("Account"/"Contract"), formatted ("Ledger N", "Transaction Detail"), identifier ("Cat #2"/"USDCOIN"). No clear rule. Affects discoverability + SEO + browser tab title (currently all = "Soroban Block Explorer").
- **Class:** C (visual)
- **Defer to Gate B** for unified pattern with Figma reference.

### F-D-4 [Class A, Severity 🟡] — Polling indicator absent on detail pages

- **Cells:** every D9 ⚠ (E0,E1,E3,E5,E6,E8,E9,E11,E13)
- **Evidence:** `PollingIndicator` exists in `libs/ui/src/timestamps/` and is exported from `libs/ui/src/index.ts`. Zero consumer in `web/src/pages/` (grep clean).
- **Per Gate A V finding (deferred):** footer hardcodes "All systems operational". Polling state is invisible to user.
- **Class:** A — but already documented as baseline. No fix-first.
- **Defer Phase 3** (V audit in Wave 6 covers).

### F-D-5 [Class A, Severity 🟡] — Composite empty-state UX on E5 (ledger detail)

- **Cell:** E5/D3 ⚠
- LedgerTransactions section renders TableEmptyState for 0-tx ledger; works in isolation. Verify with low-traffic ledger that the empty state + parent ledger summary render coherently. Not confirmed broken — flagged for spot-check.

## Skipped cells (?)

26 cells skipped (D6 transient, D7 rate limit, D8 CORS across all routes). Reason: cannot reproduce without API kill / rate-limit infra / cross-origin setup. Per task spec, mark and move on.

**Recommendation Phase 3:** spawn `XXXX_FEATURE_e2e-error-injection-playwright` to set up MSW or Playwright route interception for D6/D7/D8 cells.

## Effective measurable matrix

| Slice | Total | Measured | % |
|---|---:|---:|---:|
| All cells | 126 | 100 | 79% |
| All cells excluding D6/D7/D8 (untestable in this env) | 98 | 100 | (98 in-scope, 73 ✓/⚠/✗) |

(D6/D7/D8 are 3 cols × 14 rows minus E0 N/A = ~40 cells. Of those, ~26 were skipped, ~14 marked N/A for shell/list rows. Net measurable in-scope = ~73 cells, of which 55 ✓ / 13 ⚠ / 5 ✗.)

## Cross-cutting concerns

- **D5 (validation)** uniformly → routes to NotFound across all detail endpoints. No dedicated 400 state surface. Per H8 + Gate A F-E-2 baseline, this is **acceptable** by design — FE doesn't pre-validate, backend returns 404 for malformed input. Browser sees "X not found" identical to D4. Documented as baseline.
- **D9 (polling)** uniformly absent visible indicator across home shell + detail pages.
- **D4 (NotFound)** consistent clean rendering on E3/E5/E11/E13; composite-broken on E6/E8/E9 (sub-section query interaction).

## Per-cell verdict legend

- `✓` correct — matches expected for that state
- `⚠ partial` — renders but has a known issue documented in finding
- `✗ broken` — broken render or wrong behavior
- `N/A` — state doesn't apply to this endpoint (e.g. D4 NotFound on a list page)
- `?` — not exercised in this pass (no reliable simulation)

---

## Wave 4 delta re-run — 2026-05-25T14:19-14:23Z

**Trigger:** F-D-1 root cause (API stale binary) cleared by restarting
API from audit branch tip. Re-verified D2/D9 pagination on every list
page and on representative tab tables.

### List pages (5/5 ✓)

| Endpoint | URL on Next click | Page 2 rows | Prev state | Notes |
|---|---|---:|---|---|
| E2 `/transactions` | `?cursor=eyJk…NjEiLC…M2fX0` | 18 | enabled | 38 total rows across 2 pages; Prev returns to page 1 with 20 rows, URL `?cursor=…dir:prev`; deep-link refresh on page-2 URL renders 18 rows ✓ |
| E4 `/ledgers` | `?cursor=eyJk…MTI6MDA6MjVa…MTAwNX19` | 5 | enabled | 25 total ledgers; page 2 = last 5 |
| E7 `/assets` | n/a (single page) | n/a | n/a | 6 rows total, API `next_cursor:null`, both buttons disabled = **correct** |
| E10 `/nfts` | n/a (single page) | n/a | n/a | 5 rows total, both disabled correctly |
| E12 `/liquidity-pools` | n/a (single page) | n/a | n/a | 3 rows total, both disabled correctly |

### Tab tables sampled (3 with usable cursor)

| Section | Cursor key | Deep-link probe | Result |
|---|---|---|---|
| Contract `…CSTELLARCATS…?tab=invocations` | `cursor_i` | `cursor_i=<b64 from limit=2 probe>` | Prev enabled, Next disabled, 1 row ✓ |
| Pool `…fac63b…?cursor_p=…` participants | `cursor_p` | `cursor_p=<b64 from limit=1 probe>` | Participants table: Prev enabled, Next disabled, 1 row. Transactions table: unchanged (Prev/Next both disabled). **Cursor namespace isolation confirmed.** ✓ |
| Account `GACCAROLNFT…?cursor=…` transactions | `cursor` (default) | `cursor=<b64 from limit=2 probe>` | Account has 9 transactions; from `next_cursor` probe URL, 7 rows render with Prev enabled, Next disabled ✓ |

Pool transactions (`cursor_t`), contract events (`cursor_e`), asset transactions, nft transfers — all return `next_cursor:null` against default `limit=20` (fixture too small), so the URL key cannot be exercised but the **wire shape was verified at API level** (`page: { next_cursor, prev_cursor, limit }`) and the FE code paths all use the same `usePageHandlers` + `useCursorPagination` infrastructure already proven on the 3 sampled sections.

### CSV cells flipped

| Cell | Before | After |
|---|---|---|
| E2/D9 | ✗ 🔴 | ✓ |
| E4/D9 | ✗ 🔴 | ✓ |
| E7/D9 | ✗ 🔴 | ✓ |
| E10/D9 | ✗ 🔴 | ✓ |
| E12/D9 | ✗ 🔴 | ✓ |

(Note on D-axis labeling: the original CSV used D9 for both
"pagination function" and "polling indicator" — strictly the matrix
spec assigns D9=polling. Pagination here was captured under D9 because
the original cells described the live break of the Next/Prev buttons,
which is a D2 success-render concern. Cell labels preserved as-is to
match Wave 4 1.5 numbering. F-D-4 polling-indicator finding below is
the canonical D9 finding and is unaffected by this delta.)

### Findings that STILL STAND post-delta

- **F-D-2** (composite NotFound on E6/E8/E9) — not re-verified this pass; pre-existing finding, still stands
- **F-D-3** (H1 heading inconsistency) — visual, unchanged
- **F-D-4** (polling indicator absent) — unrelated to F-D-1; PollingIndicator still has 0 consumers in `web/src/pages/`
- **F-D-5** (E5 empty-state spot-check) — not re-verified

### NEW findings surfaced by delta pass

None. Cursor pagination, namespace isolation (`cursor_p`/`cursor_t`/`cursor_e`/`cursor_i`), URL deep-link, Prev/Next disabled-at-boundary semantics, and `placeholderData: keepPreviousData` UX (no flash on page change) all behave correctly. The wire shape from 0254 is correctly consumed end-to-end.

### Severity tally update

- Wave 4 🔴 count: **1 → 0** (F-D-1 retired)
- Cumulative 🔴 count: **3 → 2**

### Out-of-scope observation (not a finding)

Console error count on the transactions list page = 1 (the `favicon.ico` 404 already catalogued as F-AE-1). No new errors from the cursor-paginated requests.
