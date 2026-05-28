# AK — CSS theme consistency (Wave 6 / 2.6)

Grep + visual sample. Read-only.

## Hardcoded hex colors (4 instances in tsx/ts, excl. comments)

| File:line                                          | Value          | Context                                                                    | Severity                                                                     |
| -------------------------------------------------- | -------------- | -------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| `web/src/pages/contracts/ContractInterface.tsx:36` | `#155dfc`      | `const TYPE_REF_COLOR` (Solidity-type reference link blue)                 | 🟡 should be `theme.palette.info.main` or new `palette.code.typeRef` token   |
| `libs/ui/src/identifiers/CopyButton.tsx:10`        | `#000000`      | `const COPIED_ICON_COLOR` (icon color when "Copied" state)                 | 🟢 sometimes black-on-X is intentional; lift to `theme.palette.common.black` |
| `libs/ui/src/layout/PageGridBackdrop.tsx:37,39`    | `#000` (twice) | radial-gradient backdrop                                                   | 🟢 cosmetic; could use `theme.palette.background.default`                    |
| `web/src/pages/HomePage.tsx:21`                    | `#fdda24`      | in a comment only ("warm gold glow"); the actual sx prop uses theme tokens | ✓ comment only — not a finding                                               |

### F-W6-AK-1 [Class A, Severity 🟡 MEDIUM] 3 hardcoded hex constants survive — minor token-system leakage

Consolidate into theme tokens. Phase 3 micro-task.

## Z-index strategy

Grep `zIndex` in `web/src` + `libs/ui/src`: 5 occurrences.

| Location                                     | Value                | Semantic?              |
| -------------------------------------------- | -------------------- | ---------------------- |
| `web/src/pages/HomePage.tsx:34`              | `0`                  | backdrop layer         |
| `web/src/pages/HomePage.tsx:68`              | `1`                  | content above backdrop |
| `web/src/router/AppShell.tsx:170`            | `1`                  | content above backdrop |
| `libs/ui/src/layout/PageGridBackdrop.tsx:26` | `0`                  | backdrop component     |
| `libs/ui/src/layout/TopNav.tsx:168`          | `theme.zIndex.modal` | ✓ semantic             |

### F-W6-AK-2 [Class A, Severity 🟢 LOW] Z-index uses raw 0/1 ad-hoc; no defined scale

`theme.zIndex.modal` is used once correctly. The 4 raw `0`/`1` could be consolidated as `theme.zIndex.appBackdrop` (0) + `theme.zIndex.appContent` (1). Minor; cross-cite F-AH (file/folder structure).

## Spacing scale

MUI default scale (8px multiplier). Sample inspection in snippets: `mt: 2, mb: 0.5, py: 2` etc. ✓ uses MUI shorthand → multiples-of-8 enforced by theme.

### F-W6-AK-3 [Class A, Severity ✓] Spacing scale consistent (uses MUI `sx` shorthand throughout)

No finding.

## Border-radius / shadows

Not deep-audited. Sampled: most surfaces use `theme.shape.borderRadius` (MUI default 4) or explicit `borderRadius: 1` / `2`. No `shadows: ['none', '0 1px 2px ...']` raw values observed in grep.

### F-W6-AK-4 [Class A, Severity 🟢 LOW] Border-radius/shadow audit DEFERRED — no obvious anomalies in random sample

Spot-check pass acceptable. Defer Phase 3 if patterns surface elsewhere.

## CSS approach single primary

Codebase uses MUI `sx` prop + `styled()` from `@mui/system` + occasional theme overrides in `libs/ui/src/theme/overrides.ts`. **No** CSS modules, **no** Tailwind, **no** stylesheets/Plain CSS files imported.

### F-W6-AK-5 [Class A, Severity ✓] CSS approach single (MUI sx + styled); no mix

✓ Good — minimal CSS-strategy debt. No finding.

## Theme tokens usage

Grep `theme.palette` in `web/src`: 200+ hits. Theme tokens used pervasively. Sample: `color: 'text.tertiary'`, `bgcolor: 'background.paper'`, etc.

### F-W6-AK-6 [Class A, Severity 🟢 LOW] Theme tokens used pervasively; minor leakage in 3 hex constants only

Combine with F-W6-AK-1 in Phase 3.

## Summary

Theme consistency is **good**. Single CSS approach, MUI shorthand for spacing, theme palette dominant. Only blemish: 3 hardcoded hex constants in 3 components. No new fix-first; all Phase 3 micro-defers.

## design_parity update 2026-05-27 (06ab34cc)

Both AK findings **REGRESSED** by the `feat/design_parity` merge (`06ab34cc`, merge `62c988d4`). Source: `design-parity-impact-2026-05-27.md` §4 + §Regressions. New regression cards: **11.2** (hex) + **11.3** (z-index); new findings **F-DP-2** + **F-DP-3** (appendix).

- **F-W6-AK-1 / F-AK-1 (hardcoded hex): REGRESSED — 3 → 5.** `AssetIcon` (`sac` kind) now adds inline `'#724311'` + `'#fffcc2'`; `ContractInterface` `TYPE_REF_COLOR='#155dfc'` retained. design_parity was exactly the Figma-parity pass card 7.1 hoped would *close* this finding — instead it added 2 more. Fix: move the 2 new AssetIcon hex values to theme tokens (e.g. `palette.assetKind.sac`) as part of the card 7.1 / card 11.2 hex consolidation. (`CopyButton` `#000000` + `PageGridBackdrop` `#000` unchanged.)
- **F-W6-AK-2 / F-AK-2 (raw z-index): REGRESSED (mildly).** AppShell / TopNav / SecondaryNav / Footer now sprinkle raw `zIndex: 2` (layering the shell above `PageGridBackdrop`) in addition to the prior raw `0`/`1`. More ad-hoc z-index, still no defined scale. Fix: define a scale (`theme.zIndex.appBackdrop`/`appContent`/`appShell`) and migrate all raw values — card 7.1 / card 11.3.

Cross-ref: `design-parity-impact-2026-05-27.md`; F-DP-2 + F-DP-3 (appendix); cards 7.1 + 11.2 + 11.3.
