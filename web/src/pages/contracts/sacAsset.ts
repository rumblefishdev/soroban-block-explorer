import type { SacAsset } from '@rumblefish/api-types';

import { NATIVE_ASSET_CODE } from '../assets/assetType.js';

/**
 * Native XLM facet. The API contract is BOTH-null for native and BOTH-present
 * for classic — so "native" must be detected as both-null, never as "the pair
 * is incomplete". The earlier `code && issuer ? … : native` shape treated a
 * drifted row (one field missing) as XLM: a mis-route to `/assets/native` and
 * a "Native XLM" tooltip on some random asset — exactly the plausible-but-
 * wrong display the no-misleading-fallbacks rule forbids (review, 2026-08-13).
 */
function isNativeSac(sac: SacAsset): boolean {
  return sac.asset_code == null && sac.issuer == null;
}

/** Display code for a SAC's mirrored asset — native XLM carries no code.
 *  Schema drift (half a pair) shows the honest `?`, not a fake XLM. */
export function sacAssetCode(sac: SacAsset): string {
  if (isNativeSac(sac)) return NATIVE_ASSET_CODE;
  return sac.asset_code ?? '?';
}

/**
 * Asset-detail route token for a SAC's mirrored asset — the canonical
 * `CODE-ISSUER | native` shape (`routes.asset`). Returns `null` for a drifted
 * row (one of the pair missing): there is no route that can honestly be
 * built, and callers degrade to an unlinked rendering.
 */
export function sacAssetId(sac: SacAsset): string | null {
  if (isNativeSac(sac)) return 'native';
  return sac.asset_code && sac.issuer
    ? `${sac.asset_code}-${sac.issuer}`
    : null;
}

/**
 * Hover text disambiguating a linked SAC chip (task 0472) — the bare code is
 * ambiguous (prod carries many issuers of e.g. "USDC"), and the list chip has
 * no room for a labelled issuer cell like the detail row.
 */
export function sacAssetLabel(sac: SacAsset): string {
  if (isNativeSac(sac)) return 'Native XLM';
  return sac.asset_code && sac.issuer
    ? `${sac.asset_code} issued by ${sac.issuer}`
    : sac.asset_code ?? 'Unknown asset';
}
