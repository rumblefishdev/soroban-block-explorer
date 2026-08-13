import type { ChipProps } from '@rumblefish/soroban-block-explorer-ui';

export interface AssetTypeMeta {
  /** Human-readable badge label. */
  label: string;
  /** Chip colour distinguishing the asset class. */
  color: ChipProps['color'];
}

/**
 * Maps the API `asset_type_name` (`native | classic_credit | soroban`) to a
 * TYPE badge label and colour. Asset identity is the most confusing area for
 * users, so each class gets a visually distinct chip. This axis is orthogonal
 * to the SAC facet: `sac` is NOT an `asset_type_name` (ADR 0051 — a SAC is a
 * facet of a classic_credit / native row), so it is surfaced as a separate
 * {@link SAC_TAG} property tag, never as a type here.
 */
const META: Record<string, AssetTypeMeta> = {
  native: { label: 'Native', color: 'blue' },
  classic_credit: { label: 'Classic credit', color: 'neutral' },
  soroban: { label: 'Soroban', color: 'emerald' },
};

export function assetTypeMeta(typeName?: string | null): AssetTypeMeta {
  const meta = typeName ? META[typeName] : undefined;
  return meta ?? { label: typeName ?? 'Unknown', color: 'neutral' };
}

/**
 * What native lumens are called in the UI (task 0472). The ledger gives native
 * no `asset_code`, so every surface has to supply the name — and each one used
 * to spell it out for itself: the pool legs, the asset pages, the transaction
 * list, the operation humaniser and the account balances all carried their own
 * copy. One constant, so a rename is one edit.
 *
 * The empty-case behaviour deliberately stays PER SITE and is not unified with
 * it: `assetLegLabel` throws (a pool leg always has a name; its absence means
 * the API contract broke), while `assetDisplayCode` returns `null` (527 type-3
 * assets on prod genuinely have neither code nor symbol). Those are two correct
 * answers to two different questions, not an inconsistency to flatten.
 */
export const NATIVE_ASSET_CODE = 'XLM';

/**
 * True when an operation-side asset field denotes native lumens. Operations
 * carry the asset as a STRING (`'native'` | `'CODE:ISSUER'`), not as the asset
 * row shape {@link assetDisplayCode} takes — hence a separate adapter over the
 * same constant rather than one function for both.
 */
export function isNativeAssetString(value: unknown): boolean {
  return value === 'native';
}

/**
 * The label an asset is shown under — title, breadcrumb, table cell, avatar
 * letter. Native XLM carries `asset_code = null` (it has no code on the
 * ledger), so it needs the same rule the pool legs already use
 * (`assetLegLabel`): the type, not the code, names it. Soroban tokens have no
 * classic code either and fall back to the on-chain SEP-41 symbol (task 0304).
 *
 * Returns `null` when nothing names the asset, so each caller picks its own
 * empty rendering (a dash in a table, a generic title on a page).
 */
export function assetDisplayCode(asset: {
  asset_type_name?: string | null;
  asset_code?: string | null;
  symbol?: string | null;
}): string | null {
  if (asset.asset_type_name === 'native') return NATIVE_ASSET_CODE;
  return asset.asset_code ?? asset.symbol ?? null;
}

/**
 * The "SAC" property tag (ADR 0051), rendered IN ADDITION to the type badge on
 * an asset that carries a DEPLOYED Stellar Asset Contract facet (`sac_deployed`).
 * A reserved (un-deployed) SAC address gets no tag — it is not a live contract.
 */
export const SAC_TAG: AssetTypeMeta = { label: 'SAC', color: 'brown' };

/**
 * Type-filter options for the assets list (the type-chip row). "SAC" is NOT here
 * — it is a separate "Has SAC" PROPERTY toggle (ADR 0051) mapped by the list
 * page to `filter[sac]=true`, orthogonal to the asset type.
 */
export const ASSET_TYPE_FILTERS: readonly { label: string; value: string }[] = [
  { label: 'All types', value: '' },
  { label: 'Classic credit', value: 'classic_credit' },
  { label: 'Soroban', value: 'soroban' },
];
