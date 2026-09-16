---
id: '0556'
title: 'BUG: bump dev tooling off 33 open dependency advisories — vitest, nx, vite and transitive'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0554', '0090']
tags: [security, tooling, dependencies, effort-small, priority-high]
links: []
history:
  - date: '2026-09-15'
    status: backlog
    who: karolkow
    note: >
      Filed after the pnpm migration push reported 65 open Dependabot alerts on
      the default branch. Matched against `pnpm-lock.yaml` on `develop`: 33
      unique advisories still resolve to installed versions (1 critical,
      19 high, 11 medium, 2 low); the other 32 are already gone or are duplicates.
---

# Bump dev tooling off open dependency advisories

## Summary

Upgrade the build/test tooling so no installed package version falls inside
an open advisory range. All affected packages are development tooling (Nx,
Vite, Vitest, ESLint chain); the shipped SPA bundle imports none of them.
Exposure is local dev servers and CI, not production.

## Context

- Dependabot scans only `master` (default branch), still on
  `package-lock.json`: 65 open alerts, all `development` scope.
- Checked 2026-09-15 against `develop`'s `pnpm-lock.yaml` with semver range
  matching: **33 unique advisories still hit an installed version**.
- Alerts on GitHub clear only after the fix reaches `master` (release).
- Web imports (react, MUI, TanStack, prismjs) contain none of the packages
  below — basis for "not in the production bundle".

| Severity   | Package (installed → first fixed)                                                                                                    | Pulled by          | Exploit condition                                                           |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------ | ------------------ | --------------------------------------------------------------------------- |
| critical   | `vitest` 4.0.9 → 4.1.0 (medium: → 4.1.11)                                                                                            | root devDep        | Vitest UI server listening; `@vitest/ui` installed, `--ui` not used in repo |
| high       | `nx` 22.6.1 + 22.7.5 → 22.7.7                                                                                                        | root devDeps       | self-hosted remote cache (unused); medium: `nx graph` CORS                  |
| high       | `vite` 7.3.3 → 7.3.5; `postcss` 8.5.15 → 8.5.23; `esbuild` 0.27.5 → 0.28.1                                                           | build / dev server | dev-server file access                                                      |
| high       | `js-yaml` 3.14.2 / 4.1.1 → 3.15.2 / 4.3.2                                                                                            | Nx, ESLint         | CPU DoS on crafted YAML                                                     |
| high       | `brace-expansion` 1.1.13 / 2.1.1 / 5.0.6 → 1.1.16 / 2.1.2 / 5.0.7                                                                    | minimatch          | exponential expansion DoS                                                   |
| high       | `fast-uri` 3.1.2 → 3.1.6                                                                                                             | ajv (Nx)           | host confusion / SSRF                                                       |
| high       | `form-data` 4.0.5 → 4.0.6; `browserslist` 4.28.2 → 4.28.7                                                                            | axios (Nx); build  | CRLF injection; crash on crafted input                                      |
| medium/low | `axios` 1.16/1.17 → 1.18.0; `picomatch` 4.0.2 → 4.0.4; `@vitest/mocker`; `@babel/core` → 7.29.6; `baseline-browser-mapping` → 2.11.0 | transitive         | mostly local dev servers                                                    |

Also found: **two Nx versions installed** — `@nx/react` is `^22.6.1`
(resolved 22.7.5) while `nx` and the other `@nx/*` are pinned to 22.6.1.

## Implementation

- `vitest`, `@vitest/ui`, `@vitest/coverage-v8`: 4.0.9 → a 4.x release
  ≥ 4.1.11 (exact pin, as today).
- `nx` and every `@nx/*`: one exact version ≥ 22.7.7 (`@nx/react` pinned
  too); run `pnpm nx migrate` if the minor requires it.
- Transitive: `pnpm update` within existing ranges; where a parent pins a
  vulnerable version, `pnpm.overrides` in `pnpm-workspace.yaml` with a
  comment naming the advisory.
- Prefer patch releases at least a week old (same safety-over-recency rule
  as the pnpm pin in 0554).
- Re-run the range match against the new lockfile; target: 0 hits.

## Acceptance Criteria

- [ ] No installed version in `pnpm-lock.yaml` falls inside any of the 33
      advisory ranges (script output recorded here)
- [ ] One Nx version installed
- [ ] `nx run-many -t lint build typecheck test` green, web e2e green,
      `api-types:check-generated` green, CI green on `develop`
- [ ] Web build output diff explained (tooling bump may change chunk hashes)
- [ ] **Docs updated** — N/A — dependency versions only, system shape
      unchanged
- [ ] **API types regenerated** — N/A unless `@hey-api/openapi-ts` changes;
      `check-generated` proves it either way

## Notes

- Dependabot alerts on GitHub stay open until a release moves this to
  `master`.
- Related: 0090 (security audit checklist) — this task is the concrete
  dependency part, not the audit sign-off.
