import { formatAmount } from '../../format.js';

const STROOPS_PER_XLM = 10_000_000n;

export function formatFee(stroops: number): string {
  const safe = Number.isFinite(stroops) ? BigInt(Math.trunc(stroops)) : 0n;
  const whole = safe / STROOPS_PER_XLM;
  const frac = safe % STROOPS_PER_XLM;
  const fracStr = frac.toString().padStart(7, '0').replace(/0+$/, '');
  const xlm =
    fracStr.length > 0 ? `${whole.toString()}.${fracStr}` : whole.toString();
  return `${xlm} XLM`;
}

export function formatStroops(stroops: number): string {
  const safe = Number.isFinite(stroops) ? Math.trunc(stroops) : 0;
  return formatAmount(safe);
}
