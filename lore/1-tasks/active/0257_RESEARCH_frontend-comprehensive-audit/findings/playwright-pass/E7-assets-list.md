# E7 — `/assets` — Wave 6 Playwright re-pass

H1: `"Assets"`. Subtitle "All classic assets and Soroban token contracts on the Stellar network".

Two filter slots at top — both render with zero-width content (`​` / ZWSP) but NO visible label or placeholder in the snapshot text dump.
Type filter chips below: "All types / Classic / SAC / Soroban".
Table 4 columns (Token / Issuer / Contract ID / Total supply / Holders).

## Console: 0 errors / 0 warnings.

## Findings

### F-W6-E7-1 [Class C, Severity 🟡 MEDIUM] Two filter slots above table with no label/placeholder visible to user

DOM snapshot returned the placeholders as empty / zero-width strings. Could not infer the filter intent from screen-reader output. If MUI Autocomplete inputs are rendered with `<Box>` and no `<InputLabel>` / `<Typography>`, sighted users see only icons; screen-reader users hear "edit text" with no semantic context. Spot-check needed: confirm the placeholders exist visually but were stripped by `innerText` due to CSS pseudo-content or icon-only rendering.

### F-W6-E7-2 [Class C, Severity 🟢 LOW] Asset icon fallback shows "?" for Soroban-only assets

`Rumblefish Token`, `Blend Token` rows show "?" icon (no asset_code, no TOML icon). Looks unintentional / placeholder. Pattern: when asset has no icon AND no asset_code, render `"?"` glyph. A neutral generic token icon (or first letter of name) would degrade more gracefully.

**Cross-cite:** new Wave 6.

### F-W6-E7-3 [Class C, Severity 🟢 LOW] Asset detail link uses composite ID `USDCOIN-GAFF…` even for Soroban contracts

Links from this list:
- Classic asset: `/assets/USDCOIN-GAFFFRANK…XXXX` (id `6` confirmed via direct API)
- Soroban: `/assets/<id>` (numeric id from server-side surrogate)

Visible mix of polymorphic IDs in URL. Cross-cite Wave 1/3 H3 polymorphic IDs.

## Cross-entity exercises

Token cell — row click navigates to `/assets/<id>` (visible by data-discover). Issuer (`GAFF…XXXX`) → `/accounts/G…`. ✓
