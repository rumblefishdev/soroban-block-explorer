# AO — Build & deploy hygiene (1.19)

**Wave:** 2
**Stance:** senior fresh-eye, read-only
**Date:** 2026-05-25

## Summary table

| #     | Check                                                              | Verdict         | Evidence                                                                                                                                                                                                                                                                                                                       | Severity | Class |
| ----- | ------------------------------------------------------------------ | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------- | ----- |
| AO-1  | `.env.example` exists                                              | ✓ ok            | Two: `.env.example` (root, backend / infra vars) + `web/.env.example` (frontend)                                                                                                                                                                                                                                               | —        | —     |
| AO-2  | `web/.env.example` covers all `VITE_*` used                        | ✓ ok            | Only one `import.meta.env.VITE_*` reference in `web/src/`: `web/src/api/config.ts:1` reads `VITE_API_BASE_URL`. `web/.env.example` lists exactly that one var. 1:1 match                                                                                                                                                       | —        | —     |
| AO-3  | Hardcoded `localhost:9000` / `localhost:4200` / `127.0.0.1` in src | ✓ ok            | Zero hits across `web/src/`, `libs/ui/src/`. `localhost:9000` lives only in `web/.env.example` + `web/.env.development` — appropriate for dev defaults                                                                                                                                                                         | —        | —     |
| AO-4  | Production bundle `console.*` leftover                             | ✓ ok            | Zero `console.*` calls in `web/src/` + `libs/ui/src/` (confirms Wave 1 F-P-3). Bundle-level inspection deferred to post-build pass (`dist/apps/web/` only exists after `nx build`)                                                                                                                                             | —        | —     |
| AO-5  | `.gitignore` coverage                                              | ✓ ok            | Lines covering `node_modules`, `dist`, `tmp`, `out-tsc`, `coverage`, `.env`, `.env.local`, `.env.*.local`, `.nx/cache`, `.nx/installation`, `.nx/workspace-data`, `.DS_Store`, `*.log`. Also project-specific: `.claude/worktrees`, `.claude/settings.local.json`, lore session files (current-user, current-task, next-tasks) | —        | —     |
| AO-6  | Secret scan — API keys / passwords in history                      | ✓ ok            | `git log --all -p \| grep -iE` over likely patterns returned 1 hit: a doc comment about a Secrets-Manager ARN pattern (`…secret:soroban/production/mtls/lambda-api-production-aBcDeF`) — not a real secret, just an example string. No actual API keys / passwords / private keys found in history                             | —        | —     |
| AO-7  | CI workflow — typescript build/lint/typecheck gate                 | ✓ ok            | `.github/workflows/ci.yml:51-71` job `typescript` runs `nx format:check --all` + `nx run-many -t lint build typecheck` on every PR + push to master, gated by `dorny/paths-filter` so only TS-affecting PRs trigger. Solid green-gate                                                                                          | —        | —     |
| AO-8  | CI workflow — API types freshness gate (ADR check)                 | ✓ ok            | `.github/workflows/ci.yml:71-89` job `api-types-codegen` runs on changes under `crates/api/**`, `Cargo.{toml,lock}`, `libs/api-types/**`. Enforces openapi regen committed in same PR per project CLAUDE.md                                                                                                                    | —        | —     |
| AO-9  | CI workflow — pages/staging deploy                                 | ✓ ok            | `.github/workflows/deploy-board.yml` deploys "Board" to GitHub Pages on push to develop; `.github/workflows/deploy-staging.yml` deploys on `staging-*` tag or manual. **No FE production deploy workflow visible** — likely intentional pre-launch                                                                             | 🟢 LOW   | D     |
| AO-10 | CI workflow — preview/PR deploy                                    | ❌ missing      | No `deploy-preview.yml` or PR-deploy workflow. Visual / UX reviewers must run locally. Common pre-launch gap                                                                                                                                                                                                                   | 🟢 LOW   | D     |
| AO-11 | Production build version stamp injected                            | not-checked-yet | Deferred to 1.20 DN (build SHA in UI) — Vite env injection at build time                                                                                                                                                                                                                                                       | —        | —     |

## Cross-references

- **AO-3, AO-4** confirm Wave 1 F-P-3 (no console leaks) and extend with hardcoded-host check.
- **AO-8** confirms CI gate referenced by project CLAUDE.md.
- **C-17** (Wave 2) found no CORS layer in API code — orthogonal to FE deploy hygiene but blocks FE in prod from a different origin; flagged there.

## Top issues

1. **AO-10 (🟢 LOW, Class D):** no PR preview-deploy workflow. Pre-launch acceptable.
2. **AO-9 (🟢 LOW, Class D):** no FE production deploy workflow visible. Likely deferred to post-launch infra task.

Otherwise: build & deploy hygiene is **clean** across all hardline checks (env docs, hardcoded hosts, console leaks, gitignore coverage, secrets in history, CI gates).
