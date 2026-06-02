# 1.1 OpenAPI strict adherence

Date: 2026-05-25
Scope: `web/src/**`, `libs/api-types/src/**`, `libs/ui/src/**`,
`.github/workflows/ci.yml`.
Stance: senior fresh-eye, read-only.

## Verdict matrix

| Check                                                | Result  | Evidence                                                                                                                                                                                                                                                           | Severity |
| ---------------------------------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------- |
| No manual `fetch()` outside generated client         | ✓       | `grep -rn "fetch(" web/src libs/ui/src libs/api-types/src` (excl. `re`/`pre`/`isFetching`) → **0 hits**                                                                                                                                                            | —        |
| No `axios` in source                                 | ✓       | Only 1 hit in `libs/api-types/src/generated/core/utils.gen.ts:132` — a comment about an alt client library; no runtime dep. `package.json` has no `axios` dep.                                                                                                     | —        |
| No `as any`                                          | ✓       | `grep -rnE "\bas any\b"` → **0 hits** in source                                                                                                                                                                                                                    | —        |
| No `@ts-ignore` / `@ts-expect-error` / `@ts-nocheck` | ✓       | **0 hits**                                                                                                                                                                                                                                                         | —        |
| No `as unknown as` schema bypass                     | ✓ minor | 1 hit (`libs/ui/src/timestamps/useNow.ts:18` — `undefined as unknown as ReturnType<typeof setInterval>`). Cross-runtime `setInterval` type duality (browser vs node); justified.                                                                                   | 🟢 LOW   |
| No `as <ResponseType>` casts bypassing schema        | ✓       | Only 1 narrowing cast in `web/src/pages/pool-detail/PoolCharts.tsx:190` (`key as ChartMetric` — string-literal narrow from controlled dropdown). Rest are `as MuiChip`, `as RouterLink`, `as const` — import aliases.                                              | —        |
| No local re-declaration of response shapes           | ✓       | `grep -rnE "(interface\|type) [A-Z][A-Za-z]*Response\b" web/src libs/ui/src` → **0 hits**                                                                                                                                                                          | —        |
| No `: any` / `<any>` generic params                  | ✓       | `grep -rnE ": any\\b\|<any>"` (excl. generated) → **0 hits**                                                                                                                                                                                                       | —        |
| Single API entry point                               | ✓       | `web/src/api/client.ts` — sole `client.setConfig({ baseUrl })` + error interceptor. All hooks import from `@rumblefish/api-types` (generated `*Options`).                                                                                                          | —        |
| API layer structure (`web/src/api/`)                 | ✓       | `client.ts`, `config.ts`, `polling.ts`, `queryKeys.ts`, `QueryProvider.tsx`, `index.ts`, plus 25 hook files (one per endpoint group). Clean.                                                                                                                       | —        |
| Query keys 1:1 to endpoint                           | ✓       | `web/src/api/queryKeys.ts` doesn't redefine keys — it uses a `predicate` matcher on the generated `_id` head (e.g., `listTransactions`, `getAccount`). Generated client owns the key structure; this layer only groups SDK ids by Resource for invalidation.       | —        |
| Mock fixtures generated from OpenAPI / no drift      | ✓       | No persistent mock fixtures (`web/dev-mock-server.mjs` referenced in 0072 archive Issues is gitignored / not present in worktree). Real backend used. **Caveat:** if dev-only mock returns drift, no fixture sync mechanism exists.                                | 🟢 LOW   |
| `libs/api-types/src/openapi.json` exists + size sane | ✓       | 142 973 bytes, modified `May 25 09:30` (today).                                                                                                                                                                                                                    | —        |
| CI gate `API types freshness` configured             | ✓       | `.github/workflows/ci.yml:72-87`: `api-types-codegen` job, `needs: changes` (paths-filter on `crates/api/**`, `Cargo.{toml,lock}`, `libs/api-types/**`), runs `npx nx run @rumblefish/api-types:generate` then `npx nx run @rumblefish/api-types:check-generated`. | —        |
| Local `nx run @rumblefish/api-types:check-generated` | ✓       | `git diff --exit-code -- libs/api-types/src/openapi.json libs/api-types/src/generated` → exit 0, no diff. Runs `extract_openapi` (cargo) + `openapi-ts` codegen, then diff.                                                                                        | —        |

## Findings

### F-AF-1 — error interceptor swallows raw envelope shape (information loss) — 🟡 MEDIUM

`web/src/api/client.ts:11-29` converts any `error` to a JS `Error`, dropping
the typed `ErrorEnvelope` shape (ADR 0008 `{code, message}`). The body is
attached as `.body`, status as `.status`, but downstream consumers cannot
discriminate on `error.code` without `(error as Error & { body?: { code?: string } }).body?.code` —
a pattern not used anywhere in `web/src/`.

→ Recommend: expose typed helper `extractErrorCode(error: unknown): string | null`
beside `client.ts` so error-boundary / mapping logic doesn't reinvent the
envelope unwrap and stays one-source.

### F-AF-2 — interceptor uses `Object.assign(error, ...)` to mutate caught Error — 🟢 LOW

`Object.assign(error, { status })` on line 14 mutates a thrown Error
instance. Works but: throws original stack from inside `@hey-api/client`
runtime; downstream `instanceof Error` is preserved but reference identity
changes (TanStack Query may have already captured the previous form).
Consider `new Error(error.message)` consistently — at the cost of stack
trace fidelity. **Not a bug**, just a smell worth a code review note.

### F-AF-3 — `useNow.ts:18` `as unknown as` is justifiable but documented inline weakly — 🟢 LOW

```ts
handle: undefined as unknown as ReturnType<typeof setInterval>,
```

The double cast bypasses TS for the "no-handle-yet" initial state. A
`ReturnType<typeof setInterval> | undefined` typed field with explicit
guarded narrowing would carry the same runtime but no cast. Nit.

### F-AF-4 — error interceptor `envelopeMessage ?? ...` may include `[object Object]` — 🟢 LOW

If `error.message` is itself an object (e.g., backend returns
`{ message: { detail: ... } }`), `String(...)` → `[object Object]` ends up
in user-visible UI via `error.message`. Worth a defensive
`typeof message === 'string'` guard. Not observed in production today —
backend conforms to ADR 0008 shape.

## Conclusion

**OpenAPI adherence is essentially perfect at the FE → backend boundary.**

- Zero direct fetches, zero axios, zero `any` casts, zero `@ts-ignore`,
  zero locally-redeclared response types, zero schema-bypass casts.
- One legitimate `as unknown as` (cross-runtime types), one legitimate
  `as ChartMetric` (literal-string narrow).
- Generated client is the only entry point; hooks are thin `useQuery`
  wrappers over generated `*Options`.
- CI gate is wired (`api-types-codegen` job) and currently green.

The only concrete finding worth a follow-up task is **F-AF-1** (typed
envelope helper) — strictly an ergonomics improvement, not a contract
violation. This is the strongest area of the FE audit.

## Recommendations

1. **🟡 MEDIUM (F-AF-1):** Spawn `XXXX_REFACTOR_frontend-error-envelope-helper`
   — add typed `extractErrorCode(error: unknown)` next to `client.ts`.
2. **🟢 LOW (F-AF-2):** Code-review note on `Object.assign(error)` pattern;
   no spawn required.
3. **🟢 LOW (F-AF-3):** Trivial nit; bundle with next polish PR.
4. **🟢 LOW (F-AF-4):** Trivial guard; bundle with F-AF-1.
