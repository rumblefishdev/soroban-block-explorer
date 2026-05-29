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

