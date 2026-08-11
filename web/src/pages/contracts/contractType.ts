import type { ChipProps } from '@rumblefish/soroban-block-explorer-ui';

export interface ContractTypeMeta {
  label: string;
  color: ChipProps['color'];
}

/**
 * Maps the API `contract_type_name` (`token | other | nft | fungible`) to a
 * badge label + colour. Each class gets a visually distinct chip; falls back
 * to the raw type with neutral colour for unknown / null.
 */
const META: Record<string, ContractTypeMeta> = {
  token: { label: 'Token', color: 'blue' },
  nft: { label: 'NFT', color: 'brown' },
  fungible: { label: 'Fungible', color: 'emerald' },
  other: { label: 'Other', color: 'neutral' },
};

export function contractTypeMeta(typeName?: string | null): ContractTypeMeta {
  const meta = typeName ? META[typeName] : undefined;
  return meta ?? { label: typeName ?? 'Unknown', color: 'neutral' };
}

/**
 * Type-filter chips for the contracts list. Values match `filter[type]` —
 * UI labels only, the API params are untouched. The `token` bucket is
 * labelled `SAC` (task 0472): on prod every type-0 contract IS a SAC
 * (cross-tab 3,946/3,946) and its rows now carry `SAC · CODE` chips, so a
 * `Token` filter label would name a category the table never shows.
 */
export const CONTRACT_TYPE_FILTERS: readonly {
  label: string;
  value: string;
}[] = [
  { label: 'All types', value: '' },
  { label: 'SAC', value: 'token' },
  { label: 'NFT', value: 'nft' },
  { label: 'Fungible', value: 'fungible' },
  { label: 'Other', value: 'other' },
];
