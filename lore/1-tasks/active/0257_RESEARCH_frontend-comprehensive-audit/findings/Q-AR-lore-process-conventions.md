# Q+AR — Lore process + commit conventions (1.18)

**Wave:** 2
**Stance:** senior fresh-eye, read-only
**Date:** 2026-05-25

## Summary tables

### Lore-side

| #   | Check                                                                    | Verdict              | Evidence                                                                                                                                                                                                                                                                                                                                                                                                                     | Severity | Class |
| --- | ------------------------------------------------------------------------ | -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ----- |
| Q-1 | Sampled archived tasks have `## Acceptance Criteria`                     | ✓ ok                 | 4/4 sampled (0238, 0246, 0249, 0251) contain the heading. AC checked-state inspection deferred (manual per-task)                                                                                                                                                                                                                                                                                                             | —        | —     |
| Q-2 | Sampled archived tasks have `## Design Decisions` w/ From Plan + Emerged | ✓ ok                 | 4/4 sampled have both `### From Plan` + `### Emerged` sub-headings                                                                                                                                                                                                                                                                                                                                                           | —        | —     |
| Q-3 | Sampled archived tasks have `## Issues Encountered`                      | partial              | 3/4 sampled have heading; **0246 missing** (`lore/1-tasks/archive/0246_FEATURE_backend-liquidity-pools-api-extensions.md`). Backend feature task; if no issues encountered then empty heading should still exist per `_template.md`                                                                                                                                                                                          | 🟢 LOW   | D     |
| Q-4 | Cross-ref to Wave 1 A2 — 0066 frontmatter drift                          | confirmed + expanded | `lore/1-tasks/active/0066_FEATURE_frontend-tanstack-query-api-client.md` frontmatter says `status: active`; body line says `## Status: Backlog`; the most-recent history note says "Implemented under web/src/api/…". **Triple-drift:** frontmatter↔body↔reality. Also `related_adr: []` + `related_tasks: []` despite ADR 0008 (Error envelope) clearly applying to API client + the task clearly chains to 0063 (FE shell) | 🟠 HIGH  | D     |
| Q-5 | API-touching commits include openapi regen                               | ✓ ok                 | Spot-checked 3 commits (`f105f36`, `193e269`, `657b837` for tasks 0246 and 0060) — each commit's `--stat` includes `libs/api-types/src/openapi.json` + `libs/api-types/src/generated/types.gen.ts`. CI gate `API types freshness` enforces (`.github/workflows/ci.yml:71-89`)                                                                                                                                                | —        | —     |
| Q-6 | Schema/docs-touching commits update `docs/architecture/**` (ADR 0032)    | ✓ ok                 | Spot-checked 3 commits (`3f39c66`, `e13a3ca`, `94059fe`): each touches schema/infra/parsing AND updates the relevant `docs/architecture/<area>/` overview/sub-doc in the same commit. ADR 0032 gate is being honored in commit content (no CI gate enforces it though — gentleman's agreement)                                                                                                                               | 🟢 LOW   | D     |
| Q-7 | ADRs cross-referenced in task frontmatter                                | ✓ ok                 | 181/182 archived tasks have `related_adr:` field present (only missing is `CLAUDE.md`). Sample 6 ADRs (0001, 0008, 0032, 0047, 0020, 0015): each referenced by ≥5 tasks (6, 10, 15, 9, 5, 7 respectively). Healthy back-ref density                                                                                                                                                                                          | —        | —     |

### Commit side

| #    | Check                                            | Verdict                | Evidence                                                                                                                                                                                                                                                                                                                                                                                                 | Severity  | Class |
| ---- | ------------------------------------------------ | ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------- | ----- |
| AR-1 | Conventional Commits compliance                  | 81%                    | `git log --since=2026-04-01` = 1054 commits; 853 match `^(feat\|fix\|chore\|docs\|refactor\|test\|build\|ci\|perf\|revert\|style)(\(...\))?: .+`. **The 201 non-matching are essentially all `Merge pull request #X` / `Merge branch` lines** (GitHub default merge-commit text) — not a real violation since the merge commit's leaf commits are conventional. Real non-merge violations: estimated <1% | —         | —     |
| AR-2 | `lore-NNNN` scope present on lore-tagged commits | partial                | Mixed styles in use: `feat(lore-0251): …` (newer) and `feat(0228): …` / `fix(0255): …` (older / sometimes still in use). Both human-parseable, but **inconsistent**: `scripts/` or skill `lore-framework-git` likely prescribes one form. Cross-ref to lore CLAUDE.md / skill                                                                                                                            | 🟢 LOW    | D     |
| AR-3 | Commitlint config                                | ❌ missing             | `find . -maxdepth 3 -name "commitlint*"` returns no hits. No `commit-msg` hook in `.husky/` (only `pre-commit` + `pre-push`). **Conventional Commits compliance is voluntary**, enforced only by team discipline                                                                                                                                                                                         | 🟡 MEDIUM | D     |
| AR-4 | PR template references lore task                 | ❌ missing             | `.github/PULL_REQUEST_TEMPLATE.md` does not exist. No template at all — PR descriptions free-form                                                                                                                                                                                                                                                                                                        | 🟡 MEDIUM | D     |
| AR-5 | Branch naming `(type)/NNNN_slug`                 | ✓ ok                   | Sampled recent 25 branches: `research/0257_*`, `fix/0251_*`, `feat/0254_*`, `refactor/0238_*`, `feat/0239_*`, `feat/0077_*` etc. — all compliant. `claude/*` worktree branches and `backup/*` excluded from convention by design                                                                                                                                                                         | —         | —     |
| AR-6 | Husky pre-commit                                 | ✓ ok (partial)         | `.husky/pre-commit` = `npx lint-staged && npm run -s verify:staged`. Runs format + lint on staged files. Wave 1 worklog noted: "fails for status-only commits" — pre-commit will block a commit that has staged changes outside lint/format scope but where lint-staged also has nothing to do. Minor friction; not a process bug                                                                        | —         | —     |
| AR-7 | Branch protection on develop                     | not-verifiable-locally | Requires GitHub settings page review. Repo has merge commits (`Merge pull request #N`) suggesting PRs are required, but local repo cannot confirm rules. Flag for human verification                                                                                                                                                                                                                     | 🟡 MEDIUM | D     |
| AR-8 | CHANGELOG.md                                     | ❌ missing             | No `CHANGELOG.md` at repo root, `web/`, `libs/`, or `libs/*/`. Pre-launch project — likely intentional defer (release notes via Git tags / GitHub Releases later)                                                                                                                                                                                                                                        | 🟢 LOW    | D     |

## Cross-references to Wave 1

- **A2 (Wave 1 archaeology — 0066 drift):** Q-4 confirms + expands. Triple-drift: frontmatter says active, body says backlog, history note says implemented. Plus empty `related_adr` + `related_tasks`. Single most impactful lore-process repair pre-launch (1-task fix-up).
- **A3 (Wave 1 archaeology — 25/28 Future Work items un-spawned):** task-process gap — Future Work items in archived tasks have no spawning automation. Phase 3 sub-phase 3.2 is the bulk-spawn step but the _system_ does not enforce. Out of scope here; flagged for 3.5 wiki write-up.

## Top issues

1. **Q-4 (🟠 HIGH, Class D):** task 0066 triple-drift (frontmatter / body / reality + empty cross-refs). One-task fix. Cross-confirms Wave 1 A2.
2. **AR-3 (🟡 MEDIUM, Class D):** no commitlint config → Conventional Commits compliance is voluntary.
3. **AR-4 (🟡 MEDIUM, Class D):** no PR template → lore-task linking in PRs is voluntary.
4. **AR-7 (🟡 MEDIUM, Class D):** branch protection on develop not verifiable from repo — needs human check of GitHub settings.
5. **AR-8 (🟢 LOW, Class D):** no CHANGELOG.md — pre-launch defer probably fine.

## Notes

- 181/182 ADR cross-ref coverage = high lore hygiene.
- Sample 0066 drift suggests other "active" tasks may have similar staleness — recommend Phase 3 task-walker that diffs `status: active` vs body `## Status:` heading.
- Lack of commitlint + no PR template means Conventional Commits + lore-NNNN scope quality is entirely team-discipline driven. 81% compliance + low audit hit rate suggests team is doing well, but new contributors will not get error feedback.
- `lore-framework-git` skill exists (per CLAUDE.md) — should mandate one of `feat(lore-NNNN): …` vs `feat(NNNN): …` and a commitlint custom plugin would enforce.

## Post-merge update 2026-05-25 (0254 merge @ 6af74d82) — develop @ 68b40058

### New finding: Q-7 🟡 MEDIUM [Class D] — forward-link expectation mismatch (0254 ↔ 0257)

**Evidence:**

- 0254 archived task body (`lore/1-tasks/archive/0254_FEATURE_backend-prev-cursor-and-pagination-tests.md`) explicitly defers test suite to 0257:
  > "(unit + integration completion, FE Playwright CLI e2e for the 13 routes, GitHub Actions CI gate) is **deferred to task 0257** (Frontend comprehensive audit pre-launch), which has 'O testing coverage' already in scope and will spawn a precisely-scoped follow-up"
- 0257 README "Out of scope (DROPPED — spawn as follow-up tasks in Phase 3)" table:
  > `| O testing coverage | XXXX_FEATURE_frontend-testing-baseline |`
- Mismatch: 0254 author treats "O testing coverage" as IN scope of 0257 audit. 0257 actual scope DROPS it and only commits to **spawning** a separate task in Phase 3 (3.2).

**Severity / impact:**

- No work lost — test suite still gets a spawned task in Phase 3.
- But "in scope" language in 0254 misrepresents 0257's coverage.
- Reader of 0254 might expect test suite to be delivered at 0257 audit close. Actual: Phase 3 will spawn a _separate_ task that itself needs implementation later.

**Action:** Phase 3 sub-phase 3.2 spawns `XXXX_FEATURE_frontend-testing-baseline` with `related_tasks: ['0238', '0254', '0257']` to make inheritance chain explicit. Add cross-link note to the spawned task's body: "originally deferred from 0254 §Future Work + 0238 manual-QA AC".

**Class D — defer to Phase 3 bulk-spawn (spawning hygiene + cross-refs, no code change).**
