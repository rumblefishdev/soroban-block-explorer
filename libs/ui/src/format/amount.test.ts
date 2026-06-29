import { describe, expect, it } from 'vitest';

import { formatAmount, scaleByDecimals } from './amount.js';

describe('scaleByDecimals', () => {
  it('scales a raw integer string by decimals, trimming trailing zeros', () => {
    expect(scaleByDecimals('500000000000000', 7)).toBe('50000000');
    expect(scaleByDecimals('123', 7)).toBe('0.0000123');
    expect(scaleByDecimals('0', 7)).toBe('0');
    expect(scaleByDecimals('63836094715548', 6)).toBe('63836094.715548');
  });

  it('keeps Int128-scale values exact (beyond Number precision)', () => {
    // 9 followed by 30 digits — far past Number.MAX_SAFE_INTEGER.
    expect(scaleByDecimals('123456789012345678901234567890', 0)).toBe(
      '123456789012345678901234567890'
    );
  });

  it('decimals <= 0 returns the integer unchanged', () => {
    expect(scaleByDecimals('42', 0)).toBe('42');
  });

  it('returns null for null / negative / non-integer input', () => {
    expect(scaleByDecimals(null, 7)).toBeNull();
    expect(scaleByDecimals(undefined, 7)).toBeNull();
    expect(scaleByDecimals('-5', 7)).toBeNull();
    expect(scaleByDecimals('12.5', 7)).toBeNull();
    expect(scaleByDecimals('abc', 7)).toBeNull();
  });

  it('composes with formatAmount for display (raw → grouped)', () => {
    expect(formatAmount(scaleByDecimals('500000000000000', 7))).toBe(
      '50,000,000'
    );
    // null (bad/absent) flows through to an em-dash.
    expect(formatAmount(scaleByDecimals(null, 7))).toBe('—');
  });
});
