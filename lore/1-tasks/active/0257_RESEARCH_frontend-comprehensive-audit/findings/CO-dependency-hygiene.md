# 1.21 CO — Dependency hygiene

Date: 2026-05-25
Tools: `npm outdated`, `npm audit`, `npm audit --omit=dev`,
`license-checker`, lockfile heuristic for duplicate versions.

## `npm outdated` (highlights)

Total rows: ~40. Material misses:

| Package | Current | Latest | Notes |
|---|---|---|---|
| `@mui/material` | 7.3.9 | 9.0.1 | **2 major versions behind** |
| `@mui/icons-material` | 7.3.9 | 9.0.1 | **2 major** |
| `@mui/x-charts` | 9.2.0 | 9.3.0 | 1 minor |
| `react` | 19.2.4 | 19.2.6 | 2 patches |
| `react-dom` | 19.2.4 | 19.2.6 | 2 patches |
| `react-router-dom` | 7.13.2 | 7.15.1 | 2 minor |
| `@tanstack/react-query` | 5.96.1 | 5.100.14 | 4 minor |
| `@tanstack/react-query-devtools` | 5.96.1 | 5.100.14 | 4 minor |
| `vite` | 7.3.1 | 8.0.14 | **1 major behind** (also CVE — see audit) |
| `typescript` | 5.9.3 | 6.0.3 | **1 major** |
| `eslint` | 8.57.1 | 10.4.0 | **2 major** (deprecated v8 EoL) |
| `@vitejs/plugin-react` | 5.2.0 | 6.0.2 | 1 major |
| `prettier` | 2.8.8 | 3.8.3 | **1 major** (formatting drift risk) |
| `@nx/*` (10 pkgs) | 22.6.1 | 22.7.3 | 1 minor — coordinated bump |
| `aws-cdk` / `aws-cdk-lib` | 2.1116 / 2.246 | 2.1124 / 2.257 | minor (infra) |
| `@types/node` | 20.19.9 | 25.9.1 | **5 major** (intentional — pin to runtime) |

## `npm audit` summary

| Severity | Full | Prod-only (`--omit=dev`) |
|---|---|---|
| critical | 0 | 0 |
| high | 22 | 2 |
| moderate | 11 | 4 |
| low | 0 | 0 |
| **total** | **33** | **6** |

### Prod-only highs (FE-runtime impact)

| Pkg | Vuln |
|---|---|
| `fast-uri` | path traversal + host confusion (CWE-22, CVSS 7.5) |
| `lodash-es` | prototype pollution × 3 in `_.unset` / `_.omit` / `_.template` |

→ `lodash-es` only enters via `node_modules/cargo-lambda-cdk/node_modules/lodash-es` (transitive infra dep, not in FE bundle — verified: `grep -rnE "from ['\"]lodash" web/src libs/ui/src` → 0 hits). `fast-uri` source unclear without deeper trace; likely fastify-adjacent test dep, but `npm audit --omit=dev` listed it as prod, so spec must confirm.

### Dev highs of concern (build-chain)

| Pkg | Vuln |
|---|---|
| `vite` (DIRECT) | Path traversal in `.map` handling; `server.fs.deny` bypass; arbitrary file read via WebSocket — **only affects dev server**, not production CloudFront artifact |
| `axios` (via `nx`/`@module-federation/dts-plugin`) | SSRF via NO_PROXY bypass; auth bypass via prototype pollution. Build-time only. |
| `picomatch` (via `nx`/`@nx/workspace`) | ReDoS + method injection |
| `@nx/*` all DIRECT | inherit picomatch + axios chain |
| `lodash-es` (via cargo-lambda-cdk) | prototype pollution |
| `fast-uri` | path traversal |
| `@babel/plugin-transform-modules-systemjs` | arbitrary code gen on malicious input |

→ Most chain back to `nx` ↔ `@module-federation/*` graph; `nx 22.7.3` upgrade may resolve a chunk. `vite 7.3.3` (already on `wanted`) closes the 3 dev-server CVEs.

## License check (`license-checker --summary --production`)

| License | Count |
|---|---|
| MIT | 86 |
| BSD-3-Clause | 3 |
| ISC | 2 |
| UNLICENSED | 1 (own root `@rumblefish/soroban-block-explorer@0.0.0` — expected, will be set at publish) |

→ All permissive licenses. No GPL/AGPL/copyleft. Clean.

## Bundle duplicate-version risk (FE-only filter)

| Package | Versions in lock |
|---|---|
| `@mui/utils` | **`7.3.9`, `9.0.0`, `9.0.1`** — 3 copies |

Other FE-relevant pkgs (`react`, `react-dom`, `react-router-dom`,
`@tanstack/react-query`, `@mui/material`, `@mui/x-charts`,
`@mui/icons-material`, `@emotion/*`, `@hey-api/*`, `lodash*`) single
version each — clean.

The `@mui/utils` 7/9 split likely caused by transitive
`@mui/x-charts@9` (latest line) pulling `@mui/utils@9` while
`@mui/material@7` pulls `@mui/utils@7`. Likely bundles both into
runtime → bundle bloat. Resolution: bump `@mui/material` 7 → 9 (the
big-bang upgrade) or pin a single `@mui/utils` via `overrides` in
package.json.

## Findings

### F-CO-1 — Dev-server Vite CVEs (high) — 🟠 HIGH (but dev-only)

3 high-severity Vite CVEs on the current 7.3.1 pin (path traversal,
fs-deny bypass, arbitrary file read). Patched in 7.3.3 (already
`wanted`). **Action:** `npm i vite@7.3.3` and a rebuild — should not
break anything (patch bump). Prod CloudFront artifact unaffected;
only contributor dev env exposed.

### F-CO-2 — Prod-bundle `lodash-es` prototype pollution — 🟢 LOW (false positive)

Audit flags `lodash-es` as prod; trace shows entry via
`cargo-lambda-cdk` (infra). FE bundle has zero `lodash` imports
(grep verified). Recommend an `npm audit` allowlist entry once
infra path is confirmed not user-reachable.

### F-CO-3 — `@mui/material` 2 major versions behind — 🟡 MEDIUM

7 → 9 is a real upgrade lift (breaking sx changes, Grid v2, etc.).
But: 9.0.1 is the line `@mui/x-charts` and `@mui/icons-material`
already deployed for FE, so staying on `@mui/material@7` while
sibling packages move to 9 creates **3 copies of `@mui/utils`**
in the bundle (F-CO-6). Worth a coordinated bump task pre-launch.

### F-CO-4 — `react-router-dom` 7.13 → 7.15 (2 minor behind) — 🟢 LOW

No CVE. Patch in next batch.

### F-CO-5 — `eslint` v8 EoL — 🟡 MEDIUM

ESLint 8 reached end of life 2024-10-05; on 8.57.1. Latest 10.4.0.
Upgrade is invasive (flat config required for v9+, ESLint 10 dropped
Node 18). Worth scoping into a dedicated task — bundle with
`typescript-eslint 8 → 9` migration.

### F-CO-6 — `@mui/utils` triple-versioned in lock — 🟠 HIGH

`7.3.9`, `9.0.0`, `9.0.1` all present. Real bundle bloat. Resolution
path: bump `@mui/material` to 9 (F-CO-3) so the whole MUI surface
unifies. Confirmed via bundle analysis (`AI`) that index chunk is 581
KB minified, partly explained.

### F-CO-7 — No `Snyk` / `Dependabot` / `Renovate` automation — 🟢 LOW

No `.github/dependabot.yml`, no `renovate.json`, no Snyk integration
detected in `.github/workflows/`. 33 vulns will accumulate without a
push mechanism. Recommend Renovate + grouped MUI/Nx/MUI batches.

### F-CO-8 — `prettier 2 → 3` deferred indefinitely — 🟢 LOW

Latest 3.8.3. Significant formatting differences (trailing commas,
arrow parens). Drift risk if a contributor accidentally runs latest;
worth bumping in a dedicated PR with a full `format:write` follow-up.

## Recommendations

1. **🟠 HIGH (F-CO-1):** Bump Vite 7.3.1 → 7.3.3 today (dev-server CVE
   trio). Trivial patch.
2. **🟠 HIGH (F-CO-6):** Spawn `XXXX_REFACTOR_frontend-mui-7-to-9-bump` —
   eliminates `@mui/utils` triplication + pulls latest MUI security
   patches.
3. **🟡 MEDIUM (F-CO-5):** Spawn `XXXX_REFACTOR_frontend-eslint-9-flat-config`.
4. **🟢 LOW (F-CO-7):** Spawn `XXXX_FEATURE_renovate-config` or enable
   Dependabot for the FE/infra package ecosystems.
5. **🟢 LOW (F-CO-2):** Allowlist `lodash-es` via `cargo-lambda-cdk` in
   `npm audit` config (confirm not user-reachable first).
6. **🟢 LOW (F-CO-8):** Bundle prettier 2→3 with eslint bump.
