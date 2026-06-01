import { describe, expect, it } from 'vitest';

import { formatAmount } from '@rumblefish/soroban-block-explorer-ui';

describe('formatAmount', () => {
  it('returns em-dash for null/undefined', () => {
    expect(formatAmount(null)).toBe('—');
    expect(formatAmount(undefined)).toBe('—');
  });

  it('returns em-dash for empty or non-numeric strings', () => {
    expect(formatAmount('')).toBe('—');
    expect(formatAmount('   ')).toBe('—');
    expect(formatAmount('abc')).toBe('—');
    expect(formatAmount('1.2.3')).toBe('—');
  });

  it('inserts thousands separators for integers', () => {
    expect(formatAmount(0)).toBe('0');
    expect(formatAmount(1)).toBe('1');
    expect(formatAmount(1_000)).toBe('1,000');
    expect(formatAmount(1_234_567)).toBe('1,234,567');
  });

  it('trims trailing zeros from the fractional part', () => {
    expect(formatAmount('1.5000000')).toBe('1.5');
    expect(formatAmount('100.0')).toBe('100');
  });

  it('pads the fractional part up to minDecimals', () => {
    expect(formatAmount(1, 2)).toBe('1.00');
    expect(formatAmount('1.5', 2)).toBe('1.50');
    expect(formatAmount('1.234', 2)).toBe('1.234');
  });

  it('accepts numeric input', () => {
    expect(formatAmount(1234.56)).toBe('1,234.56');
  });

  it('preserves leading minus sign', () => {
    expect(formatAmount('-1234.5')).toBe('-1,234.5');
    expect(formatAmount(-1000)).toBe('-1,000');
  });
});
