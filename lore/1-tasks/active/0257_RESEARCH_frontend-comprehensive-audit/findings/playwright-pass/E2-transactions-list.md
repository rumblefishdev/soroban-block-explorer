# E2 — `/transactions` — Wave 6 Playwright re-pass

H1: `"Transactions list"` — body subtitle "All indexed transactions on the Stellar network".

Two filters: source account/contract text input + operation type combobox. Table 7 columns (Hash / Ledger / Source account / Operation / Status / Fee / Time). Pagination at bottom (Previous disabled, Next enabled). 20 rows visible.

## Console: 0 errors / 0 warnings (clean).

## Findings

### F-W6-E2-1 [Class C, Severity 🟢 LOW] Heading "Transactions list" inconsistent with side-nav label "Transactions"

Other list pages: `/assets` → h1 "Assets" (matches nav), `/ledgers` → h1 "Ledgers" (matches), `/nfts` → h1 "NFTs" (matches), `/liquidity-pools` → h1 "Liquidity Pools" (matches). Only `/transactions` adds the word "list". Minor cosmetic inconsistency.

### F-W6-E2-2 [Class A, Severity 🟢 LOW] Operation filter combobox shows "All operations type" (typo — should be "All operation types")

`generic [combobox]: All operations type` — confirmed in snapshot ref=e256. Grammar nit.

## Cross-entity exercises

All sampled row cells have working links (`<a href>`): tx hash → `/transactions/<hash>`, source account → `/accounts/<G…>`. Operation cell is text-only (no link, intentional — it's a label not navigable entity).

## Pagination state preservation

Clicked Next → URL changed to include cursor query param; refresh preserves cursor; back/forward works. ✓
