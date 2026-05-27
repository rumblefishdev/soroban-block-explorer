# E6 — `/accounts/:id` — Wave 6 Playwright re-pass

H1: `"Account"`. Sections: Summary, Balances (with empty state "No balances yet"), Recent transactions.

## Console: 0 errors / 0 warnings on valid happy path.

## Positive verification — F-D-2 fix CONFIRMED for E6

Invalid format `/accounts/GINVALID` → single NotFound block:
> "Account not found / We couldn't find anything matching this identifier. Double-check the value and try again. / GINVALID"

Valid-format-404 `/accounts/GDOESNOTEXISTXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX` → ALSO single NotFound block, identical copy. Render-gating works as Gate B fix-first #1 intended.

## Findings

### F-W6-E6-1 [Class A, Severity 🟡 MEDIUM] Sub-section queries still FIRE on 404 even though render is gated

On `/accounts/GDOESNOTEXIST...`:
```
GET /v1/accounts/GDOESNOTEXIST... → 404 (parent)
GET /v1/accounts/GDOESNOTEXIST.../transactions?limit=20 → 404 (sub)
```

Two console errors logged. The Gate B Fix-First #1 (`XXXX_BUG_composite-notfound-sub-section-queries.md`) acceptance criteria specified:
> "Loading state on parent doesn't trigger sub-section error."

The render side IS gated (no dual error blocks displayed) but the **request side is NOT** — sub-queries still hit the network. Either path (a) `enabled: !!parentData` was not added to every sub-section hook, or only added to the rendering layer. Wastes 1 network call per sub-section per failed parent + N console-error rows per failed parent navigation.

**Cross-cite:** F-D-2, F-AE-5 (Wave 1/3); Gate B Fix-First #1 — fix is partial.

### F-W6-E6-2 [Class A, Severity 🟢 LOW] NotFound has no h1 element

Heading hierarchy: no `<h1>` rendered for "Account not found". The page still uses the breadcrumb "Account / GINVA…ALID" but no level-1 heading. Affects a11y screen-reader landmark navigation. Cross-cite F-W6-NOTFOUND-1.

## Cross-entity exercises

Account ID copyable. Recent transactions rows: hash → /transactions/<hash> ✓. Empty Balances state: "Balances will appear here once network activity begins" — friendly copy. ✓

## Network

Valid: `/v1/accounts/<G>` + `/v1/accounts/<G>/transactions?limit=20` (200). Single pair, no balances endpoint hit at /accounts/<G> (might be lazy on tab visibility).
