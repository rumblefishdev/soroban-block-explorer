# E4 — `/ledgers` — Wave 6 Playwright re-pass

H1: `"Ledgers"`. Subtitle: "All indexed ledgers on the Stellar network".
Table 5 columns (Sequence / Hash / Closed at / Protocol / TX Count). Pagination at bottom.

## Console: 0 errors / 0 warnings.

## Findings

No new findings for happy path. Hash column is NOT a link (visual-only with Copy button) — consistent intent (ledger detail keyed by sequence not hash, so link from sequence makes sense). Cross-cite F-W6-E1-4 for consistency note across tables.

## Cross-entity exercises

Sequence cell → `/ledgers/<seq>` works. ✓
