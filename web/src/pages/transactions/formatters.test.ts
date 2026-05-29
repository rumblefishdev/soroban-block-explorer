import { describe, expect, it } from 'vitest';

import { formatAbsoluteUtc, formatFee } from './formatters.js';

describe('formatFee', () => {
  it('returns em-dash for non-finite input', () => {
    expect(formatFee(Number.NaN)).toBe('—');
    expect(formatFee(Number.POSITIVE_INFINITY)).toBe('—');
  });

  it('treats 0 stroops as "0 XLM"', () => {
    expect(formatFee(0)).toBe('0 XLM');
  });

  it('converts stroops to XLM and trims trailing zeros', () => {
    expect(formatFee(100)).toBe('0.00001 XLM');
    expect(formatFee(10_000_000)).toBe('1 XLM');
    expect(formatFee(25_000_000)).toBe('2.5 XLM');
    expect(formatFee(1_000)).toBe('0.0001 XLM');
  });
});

describe('formatAbsoluteUtc', () => {
  it('formats an ISO timestamp as YYYY-MM-DD HH:mm:ss UTC', () => {
    expect(formatAbsoluteUtc('2026-05-25T01:02:03Z')).toBe(
      '2026-05-25 01:02:03 UTC'
    );
  });

  it('zero-pads single-digit fields', () => {
    expect(formatAbsoluteUtc('2026-01-09T04:05:06Z')).toBe(
      '2026-01-09 04:05:06 UTC'
    );
  });

  it('returns em-dash for unparseable input', () => {
    expect(formatAbsoluteUtc('not-a-date')).toBe('—');
  });
});
