/**
 * Integer display with `en-US` thousands separators (e.g. `1,234,567`).
 * Canonical replacement for scattered inline `n.toLocaleString('en-US')`
 * calls on integer counts (ledger sequence, tx counts, stroop counts).
 */
export function formatInteger(value: number): string {
  return value.toLocaleString('en-US');
}

/**
 * Transactions-per-second display: one fixed decimal (e.g. `12.3`). Canonical
 * replacement for inline `tps.toFixed(1)`. No unit suffix.
 */
export function formatTps(value: number): string {
  return value.toFixed(1);
}

/**
 * Percentage display with two fixed decimals plus a `%` suffix (e.g.
 * `0.30%`). Canonical replacement for inline `n.toFixed(2) + '%'`.
 *
 * `fallback` is returned for non-finite input (NaN/Infinity) — callers that
 * want the raw source string on parse failure pass it here; defaults to an
 * em-dash.
 */
export function formatPercent(value: number, fallback = '—'): string {
  return Number.isFinite(value) ? `${value.toFixed(2)}%` : fallback;
}
