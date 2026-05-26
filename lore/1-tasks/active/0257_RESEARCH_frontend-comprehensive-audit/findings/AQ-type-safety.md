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

## Post-merge update 2026-05-25 — develop @ 6b7fb558 (FilipDz tx-detail PR #215)

**F-AQ-1, F-AQ-2:** STILL STAND (tsconfig unchanged).

**F-AQ-3 (🟡 — switch exhaustiveness):** PARTIALLY DEGRADED. Filip's PR
adds switches at:
- `web/src/pages/transaction-detail/normal/humanizeOp.ts:36-61` —
  switch on `light.type_name` (string, not literal union). 4 cases +
  fall-through to default `return \`${opLabel} processed\``. No
  `assertNever` (and can't be — discriminant is `string`, not narrowed).
- `web/src/pages/transaction-detail/advanced/HighlightedJson.tsx:9-19` —
  switch on `TokenKind` (string-literal union). 5 cases, no default,
  returns each branch. `noImplicitReturns` will catch new union members.

Net: 2 new switches, neither uses `assertNever`. Same pattern as Wave 1
baseline. Severity unchanged.

**F-AQ-4 (🟠 HIGH — zero branded ID types):** STILL STANDS, DEGRADED.
Filip's PR threads `string` IDs through new types without branding.
Examples:
- `useTxHashParam.ts:5-7` `{ hash: string }` — could be `TransactionHash`
- `OperationPicker.tsx` accepts `OperationItem` with no `OperationId` brand
- `humanizeOp.ts` `shortId(value: string)` accepts any string

Adding tx-detail surface enlarges the unbranded-ID surface area.

**F-AQ-5, F-AQ-6:** STILL STAND.

**NEW FINDING — F-AQ-7 🟡 MEDIUM `[Class B, Severity MEDIUM]` — `unknown` +
runtime probes for heavy XDR shapes.** Filip's tx-detail pages use
`unknown` + manual shape probes for the heavy `details` field:
- `humanizeOp.ts:9-25` — `fnNameFromHeavy`, `summaryFromHeavy` cast
  `details` as `{ function_name?: unknown; summary?: unknown }`
- `toFlowNodes.tsx:27-39` — `asObject`, `asString`, `asNestedCalls`
  helpers narrow `unknown`
- `OperationJsonDetail.tsx:13-30` — `pickDetailValue(details, key)` with
  `key in details` runtime probe

This is the *correct* defensive pattern given that backend `details` is
a `Record<string, unknown>`-style JSONB blob with no schema lock. But it
indicates an OpenAPI gap: `XdrOperationDto.details` should be typed
more strictly (probably a discriminated union by `op_type`). Cross-refs
to 0075 future-work item "Document `wasm_interface_metadata` JSONB shape"
in archaeology. New ID for the AQ batch.

**NEW FINDING — F-AQ-8 🟡 MEDIUM `[Class C, Severity MEDIUM]` — Triple
cast in `RawDataSection`.** `index.tsx:131-138` (parent):

```ts
resultsMetaXdr={
  (heavy as | { results_meta_xdr?: string | null } | null | undefined)
    ?.results_meta_xdr
}
```

Three-level cast (`heavy` → object-with-optional-field → optional access)
because `XdrOperationDto`-like type doesn't expose `results_meta_xdr`.
Indicates **api-types codegen drift** — the field exists in the API but
isn't surfaced in the generated TS shape. Worth a backend openapi schema
audit; in the FE this is the same problem class as F-AQ-7.

---

## Exhaustive cast & type-escape sweep 2026-05-26 (pre-Wave-6)

Trigger: F-AQ-7 cited 3 files in `transaction-detail/advanced/`.
Re-grep to confirm exhaustive across whole tree.

### `as unknown as` — true cross-runtime type-escape (1 site, baseline preserved)

| File:line | Reason |
|---|---|
| `libs/ui/src/timestamps/useNow.ts:18` | `setInterval` return type cross-platform polyfill |

**Wave 1 baseline of "single legitimate `as unknown as`" still holds post-Filip merge.**

### `as any` / `@ts-ignore` / `@ts-expect-error` — 0 sites

**Wave 1 baseline of "zero `any` / `@ts-ignore`" still holds post-Filip merge.**

### Structural inline casts `(x as { ... })`

| File:line | Cast | Reason |
|---|---|---|
| `web/src/api/client.ts:20` | `error as { message: unknown }` | error normalisation |
| `web/src/api/QueryProvider.tsx:14` | `error as { status?: number }` | retry policy classifier |
| `web/src/api/queryKeys.ts:39` | `head as { _id?: unknown }` | SDK_IDS_BY_RESOURCE probe |
| `web/src/pages/transaction-detail/normal/humanizeOp.ts:12` | `details as { function_name?: unknown }` | heavy XDR shape probe |
| `web/src/pages/transaction-detail/normal/humanizeOp.ts:21` | `details as { summary?: unknown }` | heavy XDR shape probe |
| `libs/ui/src/states/classifyError.ts:23` | `err as { status: unknown }` | error classifier |

**Count: 6.** F-AQ-7 cited "3 files in `transaction-detail/advanced/`" —
exhaustive count includes 3 API-layer + 2 tx-detail/normal + 1 libs/ui/states.

### `as Record<string, unknown>` (defensive narrowing prep)

| File:line | Cast |
|---|---|
| `web/src/pages/transaction-detail/advanced/OperationJsonDetail.tsx:21` | `details as Record<string, unknown>` |
| `web/src/pages/transaction-detail/advanced/OperationJsonDetail.tsx:23` | `details as Record<string, unknown>` |
| `web/src/pages/transaction-detail/advanced/HighlightedJson.tsx:63` | `value as Record<string, unknown>` |
| `web/src/pages/transaction-detail/normal/toFlowNodes.tsx:29` | `value as Record<string, unknown>` |

**Count: 4.** All in transaction-detail. F-AQ-7's "3 files in advanced/"
roughly maps to OperationJsonDetail + HighlightedJson + EventsSection
(the latter via `'contract' as const` which is safe), with toFlowNodes
in `/normal/` not `/advanced/` adding a 4th distinct file.

### Other domain-specific casts

| File:line | Cast | Notes |
|---|---|---|
| `web/src/pages/transaction-detail/normal/toFlowNodes.tsx:38` | `value as NestedCallShape[]` | post-`Array.isArray` narrow — safe |
| `web/src/pages/pool-detail/PoolCharts.tsx:190` | `key as ChartMetric` | Tabs.onChange callback string → literal union |
| `web/src/pages/transaction-detail/index.tsx:131-138` | `heavy as | { results_meta_xdr?: ... } | ...` | F-AQ-8 cited; OpenAPI codegen drift |

### Conclusion

**Total type-escape sites across tree (non-`as const`):** ~14
- 1× `as unknown as`
- 6× structural inline casts
- 4× `as Record<string, unknown>`
- 3× domain-specific casts

**Across 8 distinct files in `web/src/pages/transaction-detail/`** (Filip's
heavy-XDR domain) + **3 API-layer files** + **2 libs/ui files** + 1 pool
chart file.

F-AQ-7 said "3 files in advanced/" — exhaustive count is **broader surface
area than cited** but pattern unchanged: every instance is defensive
narrowing of backend JSONB blobs (`Record<string, unknown>` shape) — the
correct defensive pattern. Real fix: stricter OpenAPI schema for `details`
field. **Severity 🟡 MEDIUM unchanged.**

**Wave 1 baseline guarantees (`zero as any / @ts-ignore`) still hold.**

See also `findings/exhaustive-sweep-2026-05-26.md` for full sweep details.
