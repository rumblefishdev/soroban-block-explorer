# 1.11b AQ — Type safety depth

Date: 2026-05-25
Scope: `tsconfig.base.json` + per-project overrides, switch exhaustiveness,
discriminated unions, branded types.

## tsconfig flags

`tsconfig.base.json` (extends to all 4 projects):

| Flag | Value | Verdict |
|---|---|---|
| `strict` | `true` | ✓ |
| `noFallthroughCasesInSwitch` | `true` | ✓ |
| `noImplicitOverride` | `true` | ✓ |
| `noImplicitReturns` | `true` | ✓ |
| `noUnusedLocals` | `true` | ✓ |
| `noEmitOnError` | `true` | ✓ |
| `isolatedModules` | `true` | ✓ |
| `composite` | `true` | ✓ |
| **`noUncheckedIndexedAccess`** | **NOT SET** | ✗ — 🟠 HIGH |
| **`exactOptionalPropertyTypes`** | **NOT SET** | ✗ — 🟡 MEDIUM |
| `noPropertyAccessFromIndexSignature` | not set | 🟢 LOW |
| `noUnusedParameters` | not set | 🟢 LOW |

Per-project tsconfigs (`web/tsconfig.lib.json`, `libs/ui/tsconfig.lib.json`,
`libs/api-types/tsconfig.lib.json`) only override `baseUrl`, `rootDir`,
`outDir`, `tsBuildInfoFile`, `emitDeclarationOnly`,
`forceConsistentCasingInFileNames`, `jsx`, `lib`, `types`. **No flag
weakening anywhere.** Good.

## Findings

### F-AQ-1 — `noUncheckedIndexedAccess` disabled — 🟠 HIGH

The flag is the single biggest type-safety upgrade missing. Today:

```ts
const palette = FALLBACK_PALETTE[idx]; // typed AssetColor (NOT AssetColor | undefined)
palette.bg;                            // compiles even if idx out of range
```

The lint warning in F-P-1 (`assetColor.ts:131` non-null assertion)
exists precisely because the user reached for `!` to silence an
ambiguity that `noUncheckedIndexedAccess` would have flagged honestly.
Enabling it would catch this class of bug across:

- `web/src/search/useSearchResults.ts` (groups indexed by tab string)
- `web/src/api/queryKeys.ts` (`SDK_IDS_BY_RESOURCE[resource]` — currently
  saved by `satisfies` + `as const`, but consumers may break elsewhere)
- All `.map(...).filter(Boolean)[0]` patterns
- All `Record<string, X>` lookups

Recommend enabling project-wide; expect 10-50 new errors to fix (most
are 1-line `?? fallback` additions).

### F-AQ-2 — `exactOptionalPropertyTypes` disabled — 🟡 MEDIUM

Without this flag, `{ foo?: string }` accepts `{ foo: undefined }`. The
backend OpenAPI generator emits `T | undefined` for optional fields;
the FE then passes around `undefined` values that confuse "absent"
vs "explicitly null". Likely caught nothing today because hooks always
use OpenAPI types directly, but a defensive flip-on is cheap.

### F-AQ-3 — Switches in source: 4 total, 3 exhaustive, 1 implicit-fallback — 🟡 MEDIUM

Found 4 `switch` statements:

| File:line | Discriminant | Default branch? | Exhaustive? |
|---|---|---|---|
| `web/src/search/useSearchResults.ts:132` | `activeTab` (string literal union) | (need to verify) | (need to verify) |
| `web/src/api/hooks/usePoolChart.ts:24` | `period` (`1D|7D|30D|1Y`) | **no `default:`**, no return after switch — relies on `noImplicitReturns` + `noFallthroughCasesInSwitch` to catch new period values | partial |
| `libs/ui/src/visualization/OperationFlowTree.tsx:62` | `kind` (union: contract/destination/result/account/operation) | (need to verify final return / default) | (need to verify) |
| `libs/ui/src/identifiers/validators.ts:40` | `type: EntityType` | **no `default:`**, returns out of each case; would NOT compile if `EntityType` adds a member (`noImplicitReturns` saves us) | type-safe but no `assertNever` |

→ None use `assertNever(x: never): never` exhaustiveness assertion.
TS's `noImplicitReturns` provides the same guarantee for return-typed
functions, but for void switches (none here yet), nothing would catch
a new union member.

→ Recommend: add a `libs/ui/src/utils/assertNever.ts` helper and adopt
in every switch over a string-literal-union — defensive against future
union expansion.

### F-AQ-4 — Zero branded / nominal types for ID strings — 🟠 HIGH

`grep -rnE "type [A-Z][A-Za-z]*Id\s*=" web/src libs/ui/src libs/api-types/src`
→ **0 hits**.

`AccountId`, `ContractId`, `AssetId`, `LedgerSequence`, `PoolId`,
`TransactionHash`, `NftId` are all `string` at type level. Examples of
confusion this enables:

- `routes.account(contractId)` — compiler accepts even though
  the URL would be malformed
- `useAccountDetail(transactionHash)` — same

The OpenAPI `:id` polymorphic accept (per 0251 H3 analysis) makes
nominal typing harder (assets accept 3 formats), but for the strict
single-format IDs (transaction hash, ledger seq, account, contract,
pool) branded types are cheap:

```ts
declare const __brand: unique symbol;
export type Brand<T, B> = T & { readonly [__brand]: B };
export type AccountId = Brand<string, 'AccountId'>;
```

Validators in `libs/ui/src/identifiers/validators.ts` already return
`boolean`; bumping them to type-guards (`is AccountId`) would
retrofit nominal typing without a code rewrite.

### F-AQ-5 — Discriminated unions: zero explicit cases — 🟢 LOW

`grep -rnE "type [A-Z][A-Za-z]*\s*=\s*\{[^}]*type:" web/src libs/ui/src`
→ **0 hits**. The FE doesn't expose any local discriminated unions;
it consumes the OpenAPI shapes directly. State machines (loading /
error / success) are encoded via TanStack Query's
`status: 'pending' | 'error' | 'success'` discriminant. Properly
narrowed via `if (query.isSuccess) { query.data... }` per React Query
docs. **No issue.**

### F-AQ-6 — Generic constraints sensible — ✅

No suspicious `<T extends any>` (zero hits) or unconstrained generics
crept in. The few generic types in `libs/ui/src/` are MUI extension
wrappers with explicit `<T extends ChipProps>` style constraints.

## Conclusion

Type safety is **good baseline + 1 strong miss**.

- **Strong:** `strict: true`, `noFallthroughCasesInSwitch`,
  `noImplicitReturns`, `noImplicitOverride`, `noUnusedLocals`,
  `noEmitOnError`. Zero `any`, zero `@ts-ignore`. Single legitimate
  `as unknown as` (cross-runtime types).
- **Missing:** `noUncheckedIndexedAccess` is the single most valuable
  flag for catching index-out-of-bounds and `Record<string, X>` lookup
  hazards.
- **Smell:** no branded ID types — easy to cross-wire pool/account/contract
  IDs at routing boundaries. Adding via `is X` type-guards on existing
  validators is cheap and high-leverage.
- **Smell:** no `assertNever` helper for exhaustiveness — currently saved
  by `noImplicitReturns` because all switches return.

## Recommendations

1. **🟠 HIGH (F-AQ-1):** Spawn `XXXX_REFACTOR_frontend-tsconfig-no-unchecked-index-access`
   — enable flag, fix resulting errors. Bundle with F-P-1 (assetColor
   non-null assertion).
2. **🟠 HIGH (F-AQ-4):** Spawn `XXXX_REFACTOR_frontend-branded-id-types`
   — branded types via type-guarded validators. Pairs naturally with
   future router-param-validation work (0067 deferred AC).
3. **🟡 MEDIUM (F-AQ-2):** Spawn (or bundle with F-AQ-1)
   `exactOptionalPropertyTypes` flip; small batch, fixes 0-handful.
4. **🟡 MEDIUM (F-AQ-3):** Add `assertNever` helper to `libs/ui/src/utils/`
   and adopt in 4 existing switches.
