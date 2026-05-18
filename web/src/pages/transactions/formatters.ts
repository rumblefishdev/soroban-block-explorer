const STROOPS_PER_XLM = 10_000_000;

function pad(n: number): string {
  return String(n).padStart(2, '0');
}

/**
 * Formats a fee in stroops as an XLM amount, trimming trailing zeros.
 * `100` → `0.00001 XLM`, `0` → `0 XLM`.
 */
export function formatFee(stroops: number): string {
  if (!Number.isFinite(stroops)) return '—';
  const xlm = stroops / STROOPS_PER_XLM;
  const fixed = xlm.toFixed(7).replace(/\.?0+$/, '');
  return `${fixed === '' ? '0' : fixed} XLM`;
}

/**
 * Formats an ISO timestamp as `YYYY-MM-DD HH:mm:ss UTC` — the absolute
 * second line of the Transactions table Time cell.
 */
export function formatAbsoluteUtc(value: string): string {
  const d = new Date(value);
  const ms = d.getTime();
  if (!Number.isFinite(ms)) return '—';
  return (
    `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(
      d.getUTCDate()
    )} ` +
    `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(
      d.getUTCSeconds()
    )} UTC`
  );
}
