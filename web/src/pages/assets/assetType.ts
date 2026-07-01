import type { ChipProps } from '@rumblefish/soroban-block-explorer-ui';

export interface AssetTypeMeta {
  /** Human-readable badge label. */
  label: string;
  /** Chip colour distinguishing the asset class. */
  color: ChipProps['color'];
}

/**
 * Maps the API `asset_type_name` (`native | classic_credit | soroban`) to a
 * badge label and colour. Asset identity is the most confusing area for users,
 * so each class gets a visually distinct chip. `sac` is NOT an `asset_type_name`
 * (ADR 0051 — a SAC is a facet of classic_credit / native); the SAC badge is
 * derived from the `sac_contract_id` facet via {@link assetBadgeMeta}.
 */
const META: Record<string, AssetTypeMeta> = {
  native: { label: 'Native', color: 'blue' },
  classic_credit: { label: 'Classic', color: 'neutral' },
  sac: { label: 'SAC', color: 'brown' },
  soroban: { label: 'Soroban', color: 'emerald' },
};

export function assetTypeMeta(typeName?: string | null): AssetTypeMeta {
  const meta = typeName ? META[typeName] : undefined;
  return meta ?? { label: typeName ?? 'Unknown', color: 'neutral' };
}

/**
 * Badge for an asset: an asset that carries a Stellar Asset Contract facet
 * (`sac_contract_id`, ADR 0051) shows the "SAC" badge; otherwise the plain
 * `asset_type_name` badge (Native / Classic / Soroban).
 */
export function assetBadgeMeta(
  typeName?: string | null,
  sacContractId?: string | null
): AssetTypeMeta {
  if (sacContractId) return META.sac;
  return assetTypeMeta(typeName);
}

/**
 * Type-filter options for the assets list, matching the Figma filter chips. The
 * "SAC" chip is a PROPERTY filter (ADR 0051): the list page maps its `sac`
 * value to `filter[sac]=true` rather than `filter[type]=sac`.
 */
export const ASSET_TYPE_FILTERS: readonly { label: string; value: string }[] = [
  { label: 'All types', value: '' },
  { label: 'Classic', value: 'classic_credit' },
  { label: 'SAC', value: 'sac' },
  { label: 'Soroban', value: 'soroban' },
];
