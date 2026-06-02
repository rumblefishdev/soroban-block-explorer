import type { PoolAssetLeg } from '@rumblefish/api-types';
import { colorsLight } from '@rumblefish/soroban-block-explorer-ui';

/**
 * Per-leg color used by `AssetAvatar` and the matching reserve dot in
 * the pools list. Backend `PoolAssetLeg` does not (yet) ship an
 * `icon_url`, so we derive a stable color from the leg identity instead
 * of an icon. When `icon_url` lands, the avatar component will prefer
 * it and these colors become the dot/fallback only.
 *
 * Each palette entry is a `{ bg, fg, dot }` triplet from a single
 * color family in the design-system scales: `bg` is the pastel `.100`
 * tint for the avatar circle, `fg` is the dark `.900` glyph color,
 * and `dot` is the saturated mid-tone (`.500`/`.600`/`.700`) for the
 * matching reserve marker. Same hue, three brightnesses — so the
 * avatar fill and the reserve dot for a given leg always read as
 * sibling colors. Light and dark scales are identical for these
 * tints (see `theme/colors.ts`), so a single source is correct in
 * both modes.
 */
export interface AssetColor {
  bg: string;
  fg: string;
  dot: string;
}

const BLUE: AssetColor = {
  bg: colorsLight.blue[100],
  fg: colorsLight.blue[900],
  dot: colorsLight.blue[400],
};

const EMERALD: AssetColor = {
  bg: colorsLight.emerald[100],
  fg: colorsLight.emerald[900],
  dot: colorsLight.emerald[400],
};

const VIOLET: AssetColor = {
  bg: colorsLight.violet[100],
  fg: colorsLight.violet[900],
  dot: colorsLight.violet[400],
};

const GREEN: AssetColor = {
  bg: colorsLight.green[100],
  fg: colorsLight.green[900],
  dot: colorsLight.green[500],
};

const RED: AssetColor = {
  bg: colorsLight.red[100],
  fg: colorsLight.red[800],
  dot: colorsLight.red[500],
};

const YELLOW: AssetColor = {
  // Yellow at `.500`/`.600` reads as orange; using `.700` keeps the dot
  // warm-brown so it sits next to red without collapsing visually.
  bg: colorsLight.yellow[100],
  fg: colorsLight.yellow[900],
  dot: colorsLight.yellow[700],
};

const SECONDARY: AssetColor = {
  bg: colorsLight.secondary[100],
  fg: colorsLight.secondary[800],
  dot: colorsLight.secondary[600],
};

/**
 * Known-asset palette — matches Figma list page (node `266:36052`)
 * exactly: XLM avatar always blue, USDC always emerald, AQUA always
 * violet, BTC always yellow, etc. Mirrors the discrete switch-style
 * mapping used by `OperationFlowTree` rather than a random hash bucket,
 * so well-known assets read as their brand color across the explorer.
 *
 * Unknown assets (small-cap tokens not listed here) fall through to
 * `FALLBACK_PALETTE` keyed by a deterministic hash — same leg always
 * gets the same color, but the specific color is incidental.
 */
const KNOWN_ASSETS: Readonly<Record<string, AssetColor>> = {
  native: BLUE, // XLM
  XLM: BLUE,
  USDC: EMERALD,
  USDT: EMERALD,
  EURC: BLUE,
  AQUA: VIOLET,
  yXLM: BLUE,
  BTC: YELLOW,
  WBTC: YELLOW,
  ETH: SECONDARY,
  WETH: SECONDARY,
};

const FALLBACK_PALETTE: readonly AssetColor[] = [
  BLUE,
  EMERALD,
  VIOLET,
  GREEN,
  RED,
  YELLOW,
  SECONDARY,
];

/** djb2 — small, deterministic, no deps. Used only as a fallback for
 *  assets not listed in `KNOWN_ASSETS`. */
function hash(input: string): number {
  let h = 5381;
  for (let i = 0; i < input.length; i++) {
    h = ((h << 5) + h + input.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
}

/**
 * Stable identity for a leg — native XLM keys off the string `"native"`
 * (no issuer, no code on the wire), non-native off `code:issuer`. Two
 * legs with the same identity always land on the same color.
 */
function legKey(leg: PoolAssetLeg): string {
  if (leg.asset_type_name === 'native') return 'native';
  return `${leg.asset_code ?? ''}:${leg.issuer ?? ''}`;
}

export function assetLegColor(leg: PoolAssetLeg): AssetColor {
  const code =
    leg.asset_type_name === 'native' ? 'native' : leg.asset_code ?? '';
  const known = KNOWN_ASSETS[code];
  if (known) return known;
  return FALLBACK_PALETTE[hash(legKey(leg)) % FALLBACK_PALETTE.length]!;
}

/** Sugar for the reserve-dot color — same as `assetLegColor(leg).dot`,
 *  named so callsites read as their intent. */
export function reserveDotColor(leg: PoolAssetLeg): string {
  return assetLegColor(leg).dot;
}
