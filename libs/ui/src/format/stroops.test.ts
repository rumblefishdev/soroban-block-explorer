import { describe, expect, it } from 'vitest';

import { formatFee, formatTokenAmount } from './stroops.js';

describe('formatFee', () => {
  it('converts stroops to XLM and trims trailing zeros', () => {
    expect(formatFee(0)).toBe('0 XLM');
    expect(formatFee(100)).toBe('0.00001 XLM');
    expect(formatFee(1_000)).toBe('0.0001 XLM');
    expect(formatFee(10_000_000)).toBe('1 XLM');
    expect(formatFee(25_000_000)).toBe('2.5 XLM');
  });

  it('returns em-dash for non-finite / negative input', () => {
    // Negative input guards against BigInt-modulo padded-minus-sign
    // corruption; it is bad data, never a real fee.
    expect(formatFee(Number.NaN)).toBe('—');
    expect(formatFee(Number.POSITIVE_INFINITY)).toBe('—');
    expect(formatFee(-100)).toBe('—');
    expect(formatFee(-10_000_000)).toBe('—');
  });
});

describe('formatTokenAmount', () => {
  it('formats a native amount, defaulting the unit to XLM', () => {
    expect(formatTokenAmount(1_005_000_000)).toBe('100.5 XLM');
    expect(formatTokenAmount(10_000_000, null)).toBe('1 XLM');
    expect(formatTokenAmount(100, '')).toBe('0.00001 XLM');
  });

  it('uses the supplied asset code as the unit', () => {
    expect(formatTokenAmount(250_000_000, 'USDC')).toBe('25 USDC');
  });

  it('accepts a string amount and keeps large values exact, US-grouped', () => {
    // 9_000_000_000_000_000_0 stroops > Number.MAX_SAFE_INTEGER — a number
    // input would lose precision; the string path stays exact, and grouping
    // is string-based so the digits survive (task 0453 AC: US grouping).
    expect(formatTokenAmount('90071992547409910', 'XLM')).toBe(
      '9,007,199,254.740991 XLM'
    );
  });

  it('groups thousands in the integer part only', () => {
    expect(formatTokenAmount(33_831_901_066_092, 'bubba')).toBe(
      '3,383,190.1066092 bubba'
    );
  });

  it('returns null for invalid / negative / non-integer input', () => {
    expect(formatTokenAmount(Number.NaN)).toBeNull();
    expect(formatTokenAmount(-100)).toBeNull();
    expect(formatTokenAmount('12.5')).toBeNull();
    expect(formatTokenAmount('abc')).toBeNull();
  });
});
