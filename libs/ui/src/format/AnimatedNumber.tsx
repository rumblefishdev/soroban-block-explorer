import NumberFlow, { type Format } from '@number-flow/react';
import type { ReactNode } from 'react';

interface AnimatedNumberProps {
  /** Omit / `undefined` while the source data is still loading — renders
   *  `fallback` instead of the odometer. */
  value?: number;
  /**
   * `Intl.NumberFormat` options (NumberFlow's `Format` subset — notation
   * limited to standard/compact) — same contract as the string formatters
   * in this directory (`en-US` locale is fixed to match them). A function
   * is resolved against the live value, for callers whose formatting
   * depends on magnitude (e.g. compact only ≥1M).
   */
  format?: Format | ((n: number) => Format);
  /** Shown when `value` is null/undefined. Defaults to an em dash. */
  fallback?: ReactNode;
}

/**
 * Live-updating numeric display for values that change in place (TopNav
 * counters, chain-overview KPIs). NumberFlow animates only the digits
 * that actually change (odometer-style roll) and respects
 * `prefers-reduced-motion`. `tabular-nums` keeps every digit the same
 * width so updates never shift the surrounding layout.
 *
 * Not for table rows — those receive *new* elements rather than changing
 * values, which is an enter-animation concern, not a digit-roll one.
 */
export function AnimatedNumber({
  value,
  format,
  fallback = '—',
}: AnimatedNumberProps) {
  if (value == null) return <>{fallback}</>;
  return (
    <NumberFlow
      value={value}
      format={typeof format === 'function' ? format(value) : format}
      locales="en-US"
      style={{ fontVariantNumeric: 'tabular-nums' }}
    />
  );
}
