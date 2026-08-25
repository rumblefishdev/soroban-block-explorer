import { describe, expect, it } from 'vitest';

import { formatAbsoluteUtc } from './formatters.js';

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
