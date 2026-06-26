import { formatAmount } from './amount.js';

/** 1 XLM = 10,000,000 stroops. BigInt to keep large-value math exact. */
export const STROOPS_PER_XLM_BIGINT = 10_000_000n;

/**
 * Converts a stroop count to its XLM-scale decimal string (7 dp, trailing
 * zeros trimmed) using exact BigInt math. Accepts a string so on-chain amounts
 * larger than `Number.MAX_SAFE_INTEGER` stroops stay exact when the caller has
 * a string in hand. Returns `null` for non-finite / negative / non-integer
 * input — callers decide how to render the absence (em-dash, omit, …). A
 * negative value is treated as bad data (avoids a padded-minus-sign corruption
 * from BigInt modulo).
 */
function stroopsToDecimal(stroops: string | number): string | null {
  let safe: bigint;
  if (typeof stroops === 'number') {
    if (!Number.isFinite(stroops) || stroops < 0) return null;
    safe = BigInt(Math.trunc(stroops));
  } else {
    const trimmed = stroops.trim();
    if (!/^\d+$/.test(trimmed)) return null;
    safe = BigInt(trimmed);
  }
  const whole = safe / STROOPS_PER_XLM_BIGINT;
  const frac = safe % STROOPS_PER_XLM_BIGINT;
  const fracStr = frac.toString().padStart(7, '0').replace(/0+$/, '');
  return fracStr.length > 0 ? `${whole}.${fracStr}` : `${whole}`;
}

/**
 * Formats a fee in stroops as an XLM amount with unit, trimming trailing
 * zeros. BigInt arithmetic keeps large values exact. `100` → `0.00001 XLM`,
 * `0` → `0 XLM`. Non-finite or negative input → `—` (fees are never negative;
 * a negative value is bad data, not a real fee).
 */
export function formatFee(stroops: number): string {
  const xlm = stroopsToDecimal(stroops);
  return xlm == null ? '—' : `${xlm} XLM`;
}

/**
 * Formats an on-chain stroop amount as a decimal with its asset unit, e.g.
 * `1005000000 → "100.5 XLM"`, or `("250", "USDC") → "250 USDC"`. The unit
 * defaults to `XLM` (the native asset) when `assetCode` is null/empty. Returns
 * `null` when the amount is not a valid non-negative integer, so callers can
 * fall back to an amount-less label.
 *
 * Pass a string to keep values above `Number.MAX_SAFE_INTEGER` exact; a number
 * argument is already subject to double precision (lossy above 2^53 stroops,
 * ~900M XLM) before this function sees it.
 */
export function formatTokenAmount(
  stroops: string | number,
  assetCode?: string | null
): string | null {
  const decimal = stroopsToDecimal(stroops);
  if (decimal == null) return null;
  const unit = assetCode != null && assetCode.length > 0 ? assetCode : 'XLM';
  return `${decimal} ${unit}`;
}

/**
 * Formats a raw stroop count with thousands separators (no unit). Used for the
 * `(N stroops)` secondary line on fee displays. Non-finite input → `0`.
 */
export function formatStroops(stroops: number): string {
  const safe = Number.isFinite(stroops) ? Math.trunc(stroops) : 0;
  return formatAmount(safe);
}
