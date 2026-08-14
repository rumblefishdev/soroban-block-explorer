import type { PoolAssetLeg } from '@rumblefish/api-types';

import { assetColor } from '../assets/assetColor.js';
import { NATIVE_ASSET_CODE } from '../assets/assetType.js';
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
 *      search and the SAC chip all route there (task 0472).
 *   2. `asset_code` + `issuer` (classic credit) → `/assets/${code}-${issuer}`.
 *      Preferred over the SAC C-address ON PURPOSE: task 0364 dropped the
 *      SAC-facet aliasing arm from `fetch_by_contract_id` (asset_type is
 *      pinned to 3), so `/assets/{SAC C…}` 404s — verified against the API
 *      2026-08-13. The earlier contract_id-first order sent ~93k classic
 *      legs to a dead page.
 *   3. `contract_id` → `/assets/${contract_id}`. Only reachable when the
 *      code/issuer pair is incomplete; resolves for a genuine Soroban token
 *      contract, 404s for a bare SAC address — a best-effort last resort.
 *   4. Anything else (schema drift) → no link.
 */
export function legHref(leg: PoolAssetLeg): string | undefined {
  if (leg.asset_type === 0) return routes.asset('native');
  if (leg.asset_code && leg.issuer) {
    return routes.asset(`${leg.asset_code}-${leg.issuer}`);
  }
  if (leg.contract_id) return routes.asset(leg.contract_id);
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
  if (leg.asset_type_name === 'native') return NATIVE_ASSET_CODE;
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
