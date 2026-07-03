import { colorsLight } from '@rumblefish/soroban-block-explorer-ui';

/**
 * Per-asset avatar colour, shared by every surface that shows an asset:
 * the assets list / detail, account balances, and liquidity-pool legs (a
 * pool is two assets). Derived stably from the asset code, so the SAME
 * asset always reads as the SAME colour everywhere — its avatar fallback
 * fill, and the matching reserve dot on the pools view.
 *
 * Pure hash, no special-cased "known assets" and no type/kind fallback:
 * the explorer has unbounded assets, so a deterministic bucket is the only
 * thing that scales. Each palette entry is a `{ bg, fg, dot }` triplet from
 * one design-system colour family — `bg` is the pastel `.100` avatar tint,
 * `fg` the dark `.900` glyph, `dot` the saturated mid-tone for the reserve
 * marker. Light and dark scales are identical for these tints, so a single
 * source is correct in both modes.
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
  bg: colorsLight.yellow[100],
  fg: colorsLight.yellow[900],
  dot: colorsLight.yellow[700],
};
const SECONDARY: AssetColor = {
  bg: colorsLight.secondary[100],
  fg: colorsLight.secondary[800],
  dot: colorsLight.secondary[600],
};

// Shade variants of the same design-system families — wider buckets for
// list texture, all in the same pastel-bg / dark-fg / mid-dot register so
// they sit next to the base hues without clashing.
const ROSE: AssetColor = {
  bg: colorsLight.red[50],
  fg: colorsLight.red[700],
  dot: colorsLight.red[400],
};
const AMBER: AssetColor = {
  bg: colorsLight.yellow[200],
  fg: colorsLight.yellow[700],
  dot: colorsLight.yellow[500],
};
const INDIGO: AssetColor = {
  bg: colorsLight.secondary[200],
  fg: colorsLight.secondary[900],
  dot: colorsLight.secondary[700],
};
const SLATE: AssetColor = {
  bg: colorsLight.gray[200],
  fg: colorsLight.gray[800],
  dot: colorsLight.gray[500],
};

const PALETTE: readonly AssetColor[] = [
  BLUE,
  EMERALD,
  VIOLET,
  GREEN,
  RED,
  YELLOW,
  SECONDARY,
  ROSE,
  AMBER,
  INDIGO,
  SLATE,
];

/** djb2 — small, deterministic, no deps. */
function hash(input: string): number {
  let h = 5381;
  for (let i = 0; i < input.length; i++) {
    h = ((h << 5) + h + input.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
}

/**
 * Per-asset avatar colour, keyed on the displayed code (`XLM` for native,
 * the asset code otherwise). The same code reads as the same colour on the
 * assets list, asset detail, balances, and pool legs.
 */
export function assetColor(code?: string | null): AssetColor {
  const key = (code ?? '').trim().toUpperCase();
  return PALETTE[hash(key) % PALETTE.length]!;
}
