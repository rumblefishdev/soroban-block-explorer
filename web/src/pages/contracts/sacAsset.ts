import type { SacAsset } from '@rumblefish/api-types';

/** Display code for a SAC's mirrored asset — native XLM carries no code. */
export function sacAssetCode(sac: SacAsset): string {
  return sac.asset_code ?? 'XLM';
}

/**
 * Asset-detail route token for a SAC's mirrored asset — the canonical
 * `CODE-ISSUER | native` shape (`routes.asset`). The API guarantees code and
 * issuer are either both present (classic) or both null (native XLM).
 */
export function sacAssetId(sac: SacAsset): string {
  return sac.asset_code && sac.issuer
    ? `${sac.asset_code}-${sac.issuer}`
    : 'native';
}

/**
 * Hover/aria text disambiguating a linked SAC chip (task 0472) — the bare
 * code is ambiguous (prod carries many issuers of e.g. "USDC"), and the
 * list chip has no room for a labelled issuer cell like the detail row.
 */
export function sacAssetLabel(sac: SacAsset): string {
  return sac.asset_code && sac.issuer
    ? `${sac.asset_code} issued by ${sac.issuer}`
    : 'Native XLM';
}
