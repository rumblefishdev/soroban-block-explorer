import type { ChipProps } from '@rumblefish/soroban-block-explorer-ui';

import type { AssetIconKind } from './AssetIcon.js';

export interface AssetTypeMeta {
  /** Human-readable badge label. */
  label: string;
  /** Chip colour distinguishing the asset class. */
  color: ChipProps['color'];
}

/**
 * Maps the API `asset_type_name` to the colour variant on `AssetIcon`'s
 * letter avatar. Keeps the Token cell on the Assets list, the Balances
 * row on Account detail, and the Asset detail header in sync.
 */
export function iconKindFor(typeName?: string | null): AssetIconKind {
  switch (typeName) {
    case 'native':
      return 'native';
    case 'classic_credit':
      return 'classic';
    case 'sac':
      return 'sac';
    case 'soroban':
      return 'classic';
    default:
      return 'default';
  }
}

/**
 * Maps the API `asset_type_name` (`native | classic_credit | sac | soroban`)
 * to a badge label and colour. Asset identity is the most confusing area for
 * users, so each class gets a visually distinct chip.
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

/** Type-filter options for the assets list, matching the Figma filter chips. */
export const ASSET_TYPE_FILTERS: readonly { label: string; value: string }[] = [
  { label: 'All types', value: '' },
  { label: 'Classic', value: 'classic_credit' },
  { label: 'SAC', value: 'sac' },
  { label: 'Soroban', value: 'soroban' },
];
