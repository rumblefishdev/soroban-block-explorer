/**
 * What native lumens are called in the UI (task 0472). The ledger gives
 * native no `asset_code`, so every surface has to supply the name — and each
 * one used to spell it out for itself (pool legs, asset pages, transaction
 * list, operation humaniser, account balances, stroop formatting). One
 * constant, one edit on a rename.
 *
 * Lives in `libs/ui/identifiers` (app-wide identifier vocabulary, next to
 * `routeSegments`) because the format layer down here needs it too — a
 * constant defined up in `web/src/pages` can never be imported by this
 * package.
 */
export const NATIVE_ASSET_CODE = 'XLM';

/**
 * True when an operation-side asset field denotes native lumens. Operations
 * carry the asset as a STRING (`'native'` | `'CODE:ISSUER'`), not as an asset
 * row — hence an adapter over the same constant rather than one function for
 * both shapes. Typed as a narrowing predicate so a future change to the wire
 * type breaks the build instead of silently deadening call sites.
 */
export function isNativeAssetString(
  value: string | null | undefined
): value is 'native' {
  return value === 'native';
}
