import { type Format } from '@number-flow/react';
import { useEffect, useRef, useState, type ReactNode } from 'react';

interface AnimatedNumberProps {
  /** Omit / `undefined` while the source data is still loading — renders
   *  `fallback` instead of the number. */
  value?: number;
  /**
   * `Intl.NumberFormat` options (`en-US` locale is fixed to match the string
   * formatters in this directory). A function is resolved against the live
   * value, for callers whose formatting depends on magnitude (e.g. compact
   * only ≥1M).
   */
  format?: Format | ((n: number) => Format);
  /** Shown when `value` is null/undefined. Defaults to an em dash. */
  fallback?: ReactNode;
  /** When `false`, render the formatted number with NO pop animation (static).
   *  Used by the TopNav counters. Defaults to `true`. */
  animate?: boolean;
}

// --- TEST animation (task: simple scale-pop on change) ---------------------
// How much the number grows on a value change, and for how long. The whole
// formatted number scales up by this factor then eases back to 1× — entirely
// via a CSS `transition` on `transform` (no keyframes, no library).
const POP_SCALE = 1.15;
const POP_MS = 350;

/**
 * Live-updating numeric display for values that change in place (TopNav
 * counters, chain-overview KPIs).
 *
 * TEST animation: when `value` changes, the whole number gently scales up
 * (`POP_SCALE`) and transitions back to its normal size (`transition` on
 * `transform`). `tabular-nums` keeps digit widths stable; the pop is skipped
 * on first data arrival (undefined → number) so only real refreshes animate.
 */
export function AnimatedNumber({
  value,
  format,
  fallback = '—',
  animate = true,
}: AnimatedNumberProps) {
  const [popped, setPopped] = useState(false);
  const prev = useRef<number | undefined>(value);

  useEffect(() => {
    if (!animate || value == null) {
      prev.current = value;
      return undefined;
    }
    if (prev.current === value) return undefined;
    const wasNumber = prev.current != null;
    prev.current = value;
    if (!wasNumber) return undefined; // first load: no pop
    setPopped(true);
    const t = setTimeout(() => setPopped(false), POP_MS);
    return () => clearTimeout(t);
  }, [value, animate]);

  if (value == null) return <>{fallback}</>;

  const opts = typeof format === 'function' ? format(value) : format;
  const text = new Intl.NumberFormat(
    'en-US',
    opts as Intl.NumberFormatOptions
  ).format(value);

  // TopNav opts out (`animate={false}`): plain static number, no pop.
  if (!animate) {
    return <span style={{ fontVariantNumeric: 'tabular-nums' }}>{text}</span>;
  }

  return (
    <span
      style={{
        display: 'inline-block',
        fontVariantNumeric: 'tabular-nums',
        transformOrigin: 'center',
        transition: `transform ${POP_MS}ms ease-in-out`,
        transform: popped ? `scale(${POP_SCALE})` : 'scale(1)',
        willChange: 'transform',
      }}
    >
      {text}
    </span>
  );
}
