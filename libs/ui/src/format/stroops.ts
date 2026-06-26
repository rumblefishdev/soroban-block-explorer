import { formatAmount } from './amount.js';

/** 1 XLM = 10,000,000 stroops. BigInt to keep large-value math exact. */
export const STROOPS_PER_XLM_BIGINT = 10_000_000n;

/**
 * Formats a raw stroop count as a 7-decimal amount with the given unit,
 * trimming trailing zeros. BigInt arithmetic keeps large values exact.
 * `100, 'XLM'` → `0.00001 XLM`, `0, 'USDC'` → `0 USDC`. Non-finite or
 * negative input → `—` (negative is bad data, not a real amount — avoids a
 * padded-minus-sign corruption from BigInt modulo). Every Stellar asset uses
 * 7 decimals, so this works for any token, not just XLM.
 */
export function formatStroopAmount(stroops: number, unit: string): string {
  if (!Number.isFinite(stroops) || stroops < 0) return '—';
  const safe = BigInt(Math.trunc(stroops));
  const whole = safe / STROOPS_PER_XLM_BIGINT;
  const frac = safe % STROOPS_PER_XLM_BIGINT;
  const fracStr = frac.toString().padStart(7, '0').replace(/0+$/, '');
  const num = fracStr.length > 0 ? `${whole}.${fracStr}` : `${whole}`;
  return unit.length > 0 ? `${num} ${unit}` : num;
}

/**
 * Formats a fee in stroops as an XLM amount with unit. See
 * {@link formatStroopAmount}.
 */
export function formatFee(stroops: number): string {
  return formatStroopAmount(stroops, 'XLM');
}

/**
 * Formats a raw stroop count with thousands separators (no unit). Used for the
 * `(N stroops)` secondary line on fee displays. Non-finite input → `0`.
 */
export function formatStroops(stroops: number): string {
  const safe = Number.isFinite(stroops) ? Math.trunc(stroops) : 0;
  return formatAmount(safe);
}
