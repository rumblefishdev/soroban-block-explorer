import type { PoolAssetLeg } from '@rumblefish/api-types';

const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000;

/**
 * Returns the display label for one leg of a pool's asset pair.
 *
 * Native (XLM) legs come back with `asset_type_name === 'native'` and
 * `null` `asset_code`. Classic, SAC, and Soroban legs all carry a code.
 * Falls back to `?` only on schema drift.
 */
export function assetLegLabel(leg: PoolAssetLeg): string {
  if (leg.asset_type_name === 'native') return 'XLM';
  return leg.asset_code ?? '?';
}

/**
 * A pool is "stale" when its newest snapshot is older than 7 days (matches
 * the freshness window enforced by `18_get_liquidity_pools_list.sql` and
 * the participants endpoint). Stale pools come back with `null` reserves,
 * TVL, volume, and fee revenue. `participant_count` stays accurate
 * regardless of freshness (per 0246).
 */
export function isPoolStale(
  latestSnapshotAt: string | null | undefined
): boolean {
  if (!latestSnapshotAt) return true;
  const ageMs = Date.now() - new Date(latestSnapshotAt).getTime();
  return Number.isNaN(ageMs) || ageMs > SEVEN_DAYS_MS;
}

/**
 * Compact decimal display (`753.9M`, `1.2K`, `480K`) for the KPI strip.
 * Accepts the same string-or-number input as `formatAmount` and returns
 * an em-dash for null / non-numeric values.
 */
export function formatCompactAmount(
  value: string | number | null | undefined
): string {
  if (value == null) return '—';
  const n = typeof value === 'string' ? Number(value) : value;
  if (!Number.isFinite(n)) return '—';
  return new Intl.NumberFormat('en-US', {
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(n);
}
