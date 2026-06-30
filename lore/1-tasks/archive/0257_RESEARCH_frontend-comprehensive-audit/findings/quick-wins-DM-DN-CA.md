# Quick wins — DM + DN + CA (1.20)

**Wave:** 2
**Stance:** senior fresh-eye, read-only
**Date:** 2026-05-25

## Summary table

| #    | Check                                                                       | Verdict      | Evidence                                                                                                                                                                                                                                                                                                                                                                                                 | Severity            | Class                                                                                                         |
| ---- | --------------------------------------------------------------------------- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------- | ------------------------------------------------------------------------------------------------------------- |
| DM-1 | "All systems operational" footer indicator — connected to real status check | ❌ hardcoded | `libs/ui/src/layout/Footer.tsx:78-102` renders a green dot + literal text `"All systems operational"` unconditionally. No `useHealthQuery`, no `/health` or `/status` endpoint hit, no `data.status`. Always green regardless of API state. **Same bug class as V (live indicator) — confirms audit pre-hypothesis**                                                                                     | 🟠 HIGH             | A (baseline-breaker — affects 2.3 live-indicator finding + every render of every page since Footer is global) |
| DM-2 | `/health` or `/status` endpoint hit anywhere in FE                          | gap          | grep `/health`, `/status`, `healthcheck` in `web/src/` returns zero. No client-side health probe at all. (BE may expose one — orthogonal)                                                                                                                                                                                                                                                                | 🟢 LOW              | D                                                                                                             |
| DN-1 | Build version / commit SHA displayed in UI                                  | ❌ missing   | Zero grep hits for `VITE_GIT_SHA`, `VITE_BUILD`, `VITE_VERSION`, `BUILD_VERSION`, `__APP_VERSION__`, `__BUILD_SHA__`. `web/vite.config.ts` does not inject `define: { __VERSION__: … }`. `web/package.json.version` not surfaced. **No version anywhere in UI** — debugging "which build is live" is impossible                                                                                          | 🟠 HIGH             | D                                                                                                             |
| DN-2 | Build version stamped at build time (Vite `define` or env)                  | ❌ missing   | `web/vite.config.ts` has no `define` block; no `VITE_GIT_SHA` reference in env or config. Can be added trivially: `define: { __BUILD_SHA__: JSON.stringify(process.env.GITHUB_SHA ?? 'dev') }`                                                                                                                                                                                                           | 🟡 MEDIUM           | D                                                                                                             |
| CA-1 | Footer Terms of Service link → real page                                    | ❌ dead      | `libs/ui/src/layout/Footer.tsx:23-27` `LEGAL: [{ label: 'Terms of Service' }, { label: 'Privacy Policy' }, { label: 'Cookies' }]` — **no `href` field**. `FooterLink` (line 29-48) renders `component={href ? 'a' : 'span'}` so all 3 render as `<span>` (non-clickable). No `/terms`, `/privacy`, `/cookies` route or page exists in `web/src/pages/`. **All 3 are dead labels, not even broken links** | 🟠 HIGH             | C (visible to every user, every page)                                                                         |
| CA-2 | Footer Resources external links                                             | ❌ dead      | Same Footer: `RESOURCES: [{ label: 'GitHub' }, { label: 'Stellar docs' }, { label: 'Soroban docs' }, { label: 'Stellar dashboard' }]` — **no `href` field**. All 4 render as non-clickable `<span>`. Even the GitHub repo link is absent                                                                                                                                                                 | 🟠 HIGH             | C                                                                                                             |
| CA-3 | External links use `target="_blank"` + `rel="noopener noreferrer"`          | partial      | Only 1 hit in entire codebase: `web/src/pages/assets/AssetMetadata.tsx:37-38` (asset homepage link from SEP-1 TOML) uses both. **Once the Footer external links are wired with hrefs (per CA-1, CA-2), they will need `target="_blank"` + `rel="noopener noreferrer"` too**                                                                                                                              | 🟢 LOW (preventive) | D                                                                                                             |
| CA-4 | Copyright line correct                                                      | ✓ ok         | `Footer.tsx:145-146` `© {new Date().getFullYear()} Stellar Explorer. Built on the Stellar network.` Year derived dynamically. ✓                                                                                                                                                                                                                                                                          | —                   | —                                                                                                             |

## Cross-references

- **DM-1** cascades into 2.3 V (live indicator) — same anti-pattern (hardcoded status string). Pre-Wave-2 hypothesis confirmed: status surface is decorative-only.
- **DN-1, DN-2** orthogonal to Wave 1 P (no console leaks) — version/SHA stamping is positive-add, not removal.
- **CA-1, CA-2** are not "dead links" in the broken-href sense — they are **dead labels** rendered as `<span>`. Users get no feedback they're clickable, so the failure mode is silent and gentler than a 404, but legally Terms / Privacy / Cookies missing on a public site is a compliance blocker (per BZ in 0257 dropped-scope list, GDPR cookie banner is also missing — same family).

## Top issues

1. **DM-1 (🟠 HIGH, Class A):** hardcoded "All systems operational" — cascades to 2.3 V finding. Single-component fix in `Footer.tsx`, can be deferred to consume same status query as Live indicator.
2. **CA-1 (🟠 HIGH, Class C):** Terms / Privacy / Cookies legal links are dead `<span>` placeholders. **Pre-launch compliance gap** — at minimum need static placeholder pages or external links to org-wide legal docs.
3. **CA-2 (🟠 HIGH, Class C):** Resources links (GitHub / Stellar docs / Soroban docs / Stellar dashboard) are dead `<span>`. **Even the project's own GitHub link is missing.**
4. **DN-1 (🟠 HIGH, Class D):** no build version / SHA in UI. Debug-blocker post-launch.
5. **CA-3 (🟢 LOW, Class D):** preventive — when CA-1/CA-2 are wired up, ensure `target="_blank" rel="noopener noreferrer"` on external links.

## Notes

- All three quick-win areas (DM / DN / CA) point to the same root cause: **Footer was built from Figma static content without wiring data + interaction layers.** Single coherent refactor task spawn target.
- Per project CLAUDE.md `feedback_figma_first`, this would be the explicit deviation note: "Footer rendered 1:1 visual but data/interaction unwired pending follow-up task".

## Post-merge update 2026-05-25 — develop @ 6b7fb558 (FilipDz tx-detail PR #215)

Filip's PR touched `libs/ui/src/layout/Footer.tsx` (186 LOC). Spot check
confirms **layout-only refactor** (added `grid.desktop.maxWidth` /
`grid.desktop.margin` container wrapper from new `libs/ui/src/theme/grid.js`).
Data wiring unchanged.

**DM-1 (🟠 HIGH — "All systems operational" hardcoded):** STILL STANDS.
`Footer.tsx:114-116` renders the literal string unconditionally inside
the green-status pill. No `useHealthQuery`, no probe added. Same bug.

**CA-1 (🟠 HIGH — Terms / Privacy / Cookies dead `<span>`):** STILL STANDS.
`Footer.tsx:25-29` still defines:

```ts
const LEGAL: FooterNavItem[] = [
  { label: 'Terms of Service' },
  { label: 'Privacy Policy' },
  { label: 'Cookies' },
];
```

— no `href`. `FooterLink` (line 31-50) renders `component={href ? 'a' : 'span'}`
exactly as before.

**CA-2 (🟠 HIGH — Resources dead `<span>`):** STILL STANDS. Same pattern,
`Footer.tsx:18-23` — `RESOURCES` all label-only.

**DM-2 (🟢 LOW — no `/health` probe):** STILL STANDS.
**DN-1 (🟠 HIGH — no build version in UI):** STILL STANDS.
**DN-2 (🟡 MEDIUM — no vite `define` block):** STILL STANDS.
**CA-3, CA-4:** STILL STAND.

**Net:** Footer refactor was visual/layout only. All data + interaction
gaps cataloged in Wave 2 remain.
