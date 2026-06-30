# E12 — `/liquidity-pools` — Wave 6 Playwright re-pass

H1: `"Liquidity Pools"`. Subtitle "Liquidity pools on the Stellar network".
Three filter slots above table (one labeled "Any TVL"; other 2 are `​` zero-width).
Table 5 columns (Pool / Fee / Reserves / Total shares / Participants). 3 rows visible.

## Console: 0 errors / 0 warnings.

## Findings

### F-W6-E12-1 [Class C, Severity 🟡 MEDIUM] Pool ID truncation `LD5MMO…O6TL` is shown twice per row (under pair name AND in Pool column)

Visible text:

```
USDCOIN / EUR
LD5MMO...O6TL  ← truncation
```

The pool strkey is the navigation identifier, so showing the truncation under the pair name is for disambiguation. But if a row is already a link (`<a href>`), repeating the L… inside it is visual noise. Could be moved to hover/title.

### F-W6-E12-2 [Class C, Severity 🟢 LOW] "Any TVL" filter is a combobox without value — first impression is "loading state"

Cell text shows literal "Any TVL" — the placeholder/label is exposed but no visible "All TVL ranges ▾" affordance. Combobox marker needs to be visually clearer.

## Cross-entity exercises

Pool link → `/liquidity-pools/L<strkey>` ✓ (composite path uses CAP-38 canonical strkey).
Asset icons (`U`/`E`/`X`) are pictograms only — first letter — visible as small badges. Not clickable to asset detail from the LIST (only from pool DETAIL per F-K-2 fix). UX choice — verify intentional vs missing-link.
