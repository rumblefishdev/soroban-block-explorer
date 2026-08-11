import type { PoolAssetLeg } from '@rumblefish/api-types';

import { assetColor } from '../assets/assetColor.js';
import { routes } from '../../router/routes.js';

const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000;

/**
 * Resolve the cross-entity link target for a pool asset leg (task 0263).
 * Always routes to the asset detail page — backend `parse_asset_id`
 * accepts either the SAC C-strkey or a `code-issuer` composite, so both
 * classic and SAC legs resolve to the same asset row.
 *
 * Precedence:
 *   1. `asset_type === 0` (native XLM) → `/assets/native`. The reserved
 *      `native` literal IS the canonical asset token (task 0243) — the older
 *      "native has no on-chain address, so no link" rule predates it and left
 *      XLM as the only unlinkable leg in the app, while account balances,
 *      search and the SAC chip all route there (task 0472). Checked before
 *      `contract_id` on purpose: a native leg may also carry the XLM SAC
 *      mirror, and both resolve to the same row, but `native` is canonical.
 *   2. `contract_id` (SAC mirror) → `/assets/${contract_id}`.
 *   3. `asset_code` + `issuer` (classic credit) → `/assets/${code}-${issuer}`.
 *   4. Anything else (schema drift) → no link.
 */
export function legHref(leg: PoolAssetLeg): string | undefined {
  if (leg.asset_type === 0) return routes.asset('native');
  if (leg.contract_id) return routes.asset(leg.contract_id);
  if (leg.asset_code && leg.issuer) {
    return routes.asset(`${leg.asset_code}-${leg.issuer}`);
  }
  return undefined;
}

/**
 * Returns the display label for one leg of a pool's asset pair.
 *
 * Native (XLM) legs come back with `asset_type_name === 'native'` and
 * `null` `asset_code`. Classic, SAC, and Soroban legs all carry a code.
 *
 * **Hard-fail on schema drift.** If a leg has neither the native flag
 * nor an `asset_code` the backend contract is broken — throw rather
 * than silently render a `?` placeholder, so the bug is caught by the
 * surrounding `SectionErrorBoundary` instead of leaking into the UI.
 */
export function assetLegLabel(leg: PoolAssetLeg): string {
  if (leg.asset_type_name === 'native') return 'XLM';
  if (leg.asset_code != null && leg.asset_code !== '') return leg.asset_code;
  throw new Error(
    `assetLegLabel: non-native leg has no asset_code (asset_type_name=${
      leg.asset_type_name ?? 'null'
    })`
  );
}

/**
 * Reserve-dot colour for a pool leg — the saturated mid-tone of the leg's
 * per-asset colour (`assetColor`), keyed identically to the leg avatar so
 * each reserve row's dot matches its asset's avatar.
 */
export function reserveDotColor(leg: PoolAssetLeg): string {
  return assetColor(assetLegLabel(leg)).dot;
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
