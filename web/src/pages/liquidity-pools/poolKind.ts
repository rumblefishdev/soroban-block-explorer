import type { PoolKind } from '@rumblefish/api-types';
import type { ChipProps } from '@rumblefish/soroban-block-explorer-ui';

export interface PoolKindMeta {
  /** Human-readable badge label. */
  label: string;
  /** Chip colour distinguishing the protocol. */
  color: ChipProps['color'];
}

/**
 * Maps the API `pool_kind` (`classic | soroban`) to a badge label and colour.
 *
 * Keyed by the API's `PoolKind` union, so a kind without an entry is a type
 * error rather than a runtime fallback. `soroban` keeps the emerald it already
 * wears on the assets list, so one word means one colour across the app.
 */
export const POOL_KIND_META: Record<PoolKind, PoolKindMeta> = {
  classic: { label: 'Classic', color: 'neutral' },
  soroban: { label: 'Soroban', color: 'emerald' },
};

/**
 * Kind-filter options for the pools list (the chip row), mirroring
 * `ASSET_TYPE_FILTERS`. Worth filtering on because the two kinds are genuinely
 * different objects behind a shared list: a classic pool is a ledger entry with
 * a protocol-fixed fee, a Soroban one is a contract whose fee its factory chose.
 */
export const POOL_KIND_FILTERS: readonly { label: string; value: string }[] = [
  { label: 'All pools', value: '' },
  { label: 'Classic', value: 'classic' },
  { label: 'Soroban', value: 'soroban' },
];
