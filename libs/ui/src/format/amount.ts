/** Inserts thousands separators into a run of digits. */
function groupDigits(digits: string): string {
  return digits.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

/**
 * Formats a numeric value (a number or a fixed-precision decimal string) with
 * thousands separators. Trailing-zero decimals are trimmed; `minDecimals`
 * pads the fraction up to a minimum — used for token balances that should
 * always show cents-style precision. Non-numeric input renders as an em-dash.
 */
export function formatAmount(
  value: string | number | null | undefined,
  minDecimals = 0
): string {
  if (value == null) return '—';
  const raw = typeof value === 'number' ? String(value) : value.trim();
  if (raw === '' || !/^-?\d+(\.\d+)?$/.test(raw)) return '—';
  const negative = raw.startsWith('-');
  const [intPart, fracRaw = ''] = raw.replace(/^-/, '').split('.');
  let frac = fracRaw.replace(/0+$/, '');
  while (frac.length < minDecimals) frac += '0';
  return `${negative ? '-' : ''}${groupDigits(intPart)}${
    frac ? `.${frac}` : ''
  }`;
}

/**
 * Scales a RAW integer amount (string or number) by `decimals` into a decimal
 * string with trailing zeros trimmed — e.g. `("500000000000000", 7)` →
 * `"50000000"`, `("123", 7)` → `"0.0000123"`. BigInt keeps `Int128` token
 * supplies / balances exact (raw amounts exceed `Number` precision). The API
 * returns raw integers for all asset types (task 0331 Option C); callers pipe
 * the result through `formatAmount` for display. `decimals <= 0` returns the
 * integer unchanged. Returns `null` for null / negative / non-integer input so
 * `formatAmount(null)` renders an em-dash.
 */
export function scaleByDecimals(
  value: string | number | null | undefined,
  decimals: number
): string | null {
  if (value == null) return null;
  // Reject invalid decimals up front: null / undefined / NaN / fractional would
  // throw in `BigInt(decimals)`, and `null <= 0` is `true` (would silently return
  // the raw integer unscaled).
  if (!Number.isInteger(decimals) || decimals < 0) return null;
  let safe: bigint;
  if (typeof value === 'number') {
    // Reject non-integer (and non-finite) numbers rather than truncating — matches
    // the string path's `/^\d+$/` and the JSDoc "null for non-integer" contract.
    if (!Number.isInteger(value) || value < 0) return null;
    safe = BigInt(value);
  } else {
    const trimmed = value.trim();
    if (!/^\d+$/.test(trimmed)) return null;
    safe = BigInt(trimmed);
  }
  if (decimals === 0) return safe.toString();
  const scale = 10n ** BigInt(decimals);
  const whole = safe / scale;
  const frac = (safe % scale).toString().padStart(decimals, '0').replace(/0+$/, '');
  return frac.length > 0 ? `${whole}.${frac}` : `${whole}`;
}

/** Module-level — Intl.NumberFormat construction is expensive enough
 *  to be worth caching across renders. */
const COMPACT_FORMATTER = new Intl.NumberFormat('en-US', {
  notation: 'compact',
  maximumFractionDigits: 1,
});

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
  return COMPACT_FORMATTER.format(n);
}
