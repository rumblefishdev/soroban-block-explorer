# F + CH — A11y visual + auto + color blindness (Wave 6 / 2.5)

## Manual + Playwright a11y observations

### F-W6-NOTFOUND-1 [Class C, Severity 🟡 MEDIUM] NotFound pages missing h1 on 4 of 5 detail routes

| Route                        | NotFound h1          |
| ---------------------------- | -------------------- |
| `/transactions/<invalid>`    | ❌ none              |
| `/ledgers/<invalid>`         | ❌ none              |
| `/accounts/<invalid>`        | ❌ none              |
| `/contracts/<invalid>`       | ✓ "Contract" present |
| `/liquidity-pools/<invalid>` | ❌ none              |

Screen-reader users navigating by heading shortcut land mid-content with no level-1 anchor; visual users still see breadcrumb + "X not found" text but a11y tree is incomplete. Inconsistent across routes — pick a single pattern. Recommended: every NotFound page renders `<h1>{entityType} not found</h1>` or similar.

**Cross-cite:** F-W6-E3-3, F-W6-E5-, F-W6-E6-2, F-W6-E9-3, F-W6-E13-2.

### F-W6-NOTFOUND-2 [Class A, Severity 🟡 MEDIUM] Sub-section queries fire on parent 404, producing extra console 404 noise

See F-W6-E6-1 + F-W6-E9-1 + F-W6-E13- "Network requests". Gate B Fix-First #1 closed the visual side; request side still leaks. Partial fix.

### F-W6-F-1 [Class C, Severity 🟡 MEDIUM] NFT detail page has h1 but NO h2/h3 section headings

`/nfts/<C>/<token>`: `[...document.querySelectorAll('h2,h3')] → []`. Section labels (Details / Traits / Transfer history) rendered as styled `<div>` or `<Typography>` without `component=` prop. Cross-cite F-W6-E11-1.

### F-W6-F-2 [Class C, Severity 🟡 MEDIUM] Filter slots on /assets (2) + /nfts (4) + /liquidity-pools (3) lack accessible names / placeholder

Multiple `<input>` elements rendered with `​` (zero-width) labels per accessibility tree. SR users hear "edit text" with no context. Cross-cite F-W6-E7-1, F-W6-E10-1, F-W6-E12-1.

### F-W6-F-3 [Class A, Severity 🟢 LOW] First Tab focus lands on header search; focus ring visible

Confirmed: Tab from page-load → header search input → `outline: rgb(26, 26, 26) none 3px` (visible). ✓ Good focus indicator on inputs at least.

### F-W6-F-4 [Class C, Severity 🟢 LOW] One input lacks aria-label and id (header search, first inputLabels item)

```js
inputLabels: [
  {
    label: null,
    placeholder: 'Search by TX hash, accounts, contract, token',
    id: '',
  }, // ← header
  {
    label: 'Search by TX hash, accounts, contract, token',
    placeholder: '...',
    id: '',
  }, // ← hero
];
```

Header search has placeholder but no aria-label and no `<label for>`. Hero search has aria-label. Inconsistent.

### F-W6-F-5 [Class A, Severity 🟢 LOW] Copy buttons all have proper aria-label "Copy to clipboard" ✓

### F-W6-F-6 [Class C, Severity 🟢 LOW] Lighthouse a11y audit NOT RUN in this pass

Pure manual + DOM-evaluation walkthrough. Recommended: run `lighthouse --only-categories=accessibility` against `http://localhost:4200/` and each main route for automated WCAG check. Defer to Phase 3 dedicated a11y task `XXXX_FEATURE_a11y-lighthouse-baseline-pass`.

### F-W6-CH-1 [Class C, Severity 🟡 MEDIUM] Status badges (Success / Failed) rely on color but DO include icon + text

Sampled E2 transactions table: every status cell has `[ref] generic: "Success"` or `"Failed"` plus a colored dot/chip background.

Inspection: tx row 1 ("Account Merge / Success") and row 3 ("Payment / Failed") — both show:

- Text label visible (`Success` / `Failed`)
- Background chip color (green for Success, red for Failed assumed)
- **No explicit checkmark / X icon**

So mid-grade compliance: text label provides fallback for color-blind users, but no shape-cue (icon) reinforces. Protanopia-friendly enough due to text but a checkmark/X icon would meet best practice.

**Cross-cite:** new Wave 6. Defer Phase 3 small-batch.

### F-W6-CH-2 [Class C, Severity 🟢 LOW] Operation type chips on `/transactions` rely on text only (no color cue, which is OK)

Operation cells (Account Merge, Payment, Clawback, etc.) are simple text without color. This is fine for a11y but the team may want to add semantic color groups (payment-like = blue, contract-like = purple) for visual scannability.

### F-W6-F-7 [Class C, Severity 🟢 LOW] Reduced-motion / `prefers-reduced-motion` NOT verified

Did not test with OS-level reduced-motion setting in Playwright. The 14 CSS transitions noted in F-W6-AG-3 should respect `@media (prefers-reduced-motion: reduce)` and shorten to ~0ms. Spot-check needed.

### F-W6-F-8 [Class C, Severity 🟢 LOW] No keyboard trap test on dialogs/modals

No modal routes were exercised in this session — the explorer doesn't appear to use modal-based UX heavily. If TanStack devtools modal or any pop-over exists, focus trap should be confirmed.

## Color contrast spot-check

Not done programmatically (would require a CSS color sampler). Visual scan: dark text on light card backgrounds appears > 4.5:1; muted "text.tertiary" used in footer / placeholders looks marginal. Phase 3 Lighthouse run will produce the report.

## Summary

8 new Wave 6 a11y/cb findings, all 🟡 MEDIUM or 🟢 LOW. None gate-blocking; all Phase 3 defer. Key issues: NotFound h1 inconsistency (write task), unlabeled filter slots, NFT detail missing h2/h3.

## design_parity update 2026-05-27 (06ab34cc)

Source: `design-parity-impact-2026-05-27.md` §Stale-findings + §4. Maps to queue cards **7.4** (stale) + **7.1** (chips/badges, untouched).

- **F-W6-F-2 / F-W6-E7-1 / F-W6-E10-1 (filter slots lack accessible names): STALE — already fixed pre-merge, NOT a design_parity closure.** The filter inputs already carried `aria-label` + `placeholder` at `06ab34cc^` (the commit's own parent) — verified by reading the pre-merge AssetFilters/NftFilters and the untouched PoolsFilterBar. design_parity only added responsive widths. So card 7.4's a11y finding was already resolved by an earlier batch (likely Gate B); the Wave 6 finding is stale as written. **Recommend:** re-verify against current develop and downgrade card 7.4 to "verify-only" / archive. Appendix marks these rows DONE (already-fixed). Residual to confirm: header-search aria-label (F-W6-F-4).
- **F-W6-CH-1 (status badges no shape icon): UNTOUCHED.** No checkmark/X icon added by `06ab34cc`.
- **F-W6-CH-2 (op-type chips text-only): PARTIAL/tangential.** `06ab34cc` adds NEW semantic chips (Classic/SAC on AssetsTable + AccountBalances; protocol_version on LedgersTable) — but not the op-type-on-transactions grouping this finding asked for.

Cross-ref: `design-parity-impact-2026-05-27.md`; cards 7.1 + 7.4.
