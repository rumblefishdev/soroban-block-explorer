function pad(n: number): string {
  return String(n).padStart(2, '0');
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
