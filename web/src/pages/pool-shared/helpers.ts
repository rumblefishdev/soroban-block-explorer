import type { PoolAssetLeg } from '@rumblefish/api-types';

import { assetColor } from '../assets/assetColor.js';
import {
  assetDisplayCode,
  UNREGISTERED_TOKEN_LABEL,
} from '../assets/assetType.js';
import { routes } from '../../router/routes.js';

/**
 * Resolve the cross-entity link target for a pool asset leg (task 0263).
 * Always routes to the asset detail page — backend `parse_asset_id`
 * accepts either the SAC C-strkey or a `code-issuer` composite, so both
 * classic and SAC legs resolve to the same asset row.
 *
 * Precedence:
 *   1. `asset_type_name === 'native'` (native XLM) → `/assets/native`. The reserved
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
  if (leg.asset_type_name === 'native') return routes.asset('native');
  if (leg.asset_code && leg.issuer) {
    return routes.asset(`${leg.asset_code}-${leg.issuer}`);
  }
  if (leg.contract_id) return routes.asset(leg.contract_id);
  return undefined;
}

/**
 * The display label for one leg — the app-wide {@link assetDisplayCode} ladder.
 * Native is named by its type, a classic leg by its code, a soroban leg by its
 * symbol or, failing that, by its own contract address.
 *
 * A leg none of those name is a token the registry never got a row for, not
 * schema drift: one live pool holds one (production, 2026-09-22). Throwing
 * here reached the root error boundary — the list and the detail header render
 * outside any section boundary — and blanked the whole app.
 */
export function assetLegLabel(leg: PoolAssetLeg): string {
  return assetDisplayCode(leg) ?? UNREGISTERED_TOKEN_LABEL;
}

/**
 * The pool's name — its legs' labels, in registration order. Two for a classic
 * pool, up to four for a soroban one, so the separator repeats rather than
 * joining a fixed left and right.
 */
export function poolLabel(legs: readonly PoolAssetLeg[]): string {
  return legs.map(assetLegLabel).join(' / ');
}

/**
 * Reserve-dot colour for a pool leg — the saturated mid-tone of the leg's
 * per-asset colour (`assetColor`), keyed identically to the leg avatar so
 * each reserve row's dot matches its asset's avatar.
 */
export function reserveDotColor(leg: PoolAssetLeg): string {
  return assetColor(assetLegLabel(leg)).dot;
}
