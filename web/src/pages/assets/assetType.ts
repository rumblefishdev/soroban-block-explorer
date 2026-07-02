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
  classic_credit: { label: 'Classic', color: 'neutral' },
  soroban: { label: 'Soroban', color: 'emerald' },
};

export function assetTypeMeta(typeName?: string | null): AssetTypeMeta {
  const meta = typeName ? META[typeName] : undefined;
  return meta ?? { label: typeName ?? 'Unknown', color: 'neutral' };
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
  { label: 'Classic', value: 'classic_credit' },
  { label: 'Soroban', value: 'soroban' },
];
