import { describe, expect, it } from 'vitest';

import { getDefaultTruncation, truncateMiddle } from './truncate.js';

describe('truncateMiddle', () => {
  it('returns the value unchanged when prefix and suffix are both zero', () => {
    expect(truncateMiddle('abcdef', { prefix: 0, suffix: 0 })).toBe('abcdef');
  });

  it('returns the value unchanged when it fits within prefix+suffix+ellipsis', () => {
    // 6 prefix + 4 suffix + 3 ellipsis = 13; "abcdefghij" is 10 chars.
    expect(truncateMiddle('abcdefghij', { prefix: 6, suffix: 4 })).toBe(
      'abcdefghij'
    );
  });

  it('truncates with middle ellipsis once the value exceeds the budget', () => {
    expect(
      truncateMiddle(
        'GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOUJ3K4MOMNGEE36',
        {
          prefix: 4,
          suffix: 4,
        }
      )
    ).toBe('GDQP...EE36');
  });

  it('handles asymmetric prefix/suffix counts', () => {
    expect(truncateMiddle('0123456789ABCDEF', { prefix: 6, suffix: 2 })).toBe(
      '012345...EF'
    );
  });
});

describe('getDefaultTruncation', () => {
  it('returns the per-entity Figma defaults', () => {
    expect(getDefaultTruncation('transaction')).toEqual({
      prefix: 6,
      suffix: 4,
    });
    expect(getDefaultTruncation('account')).toEqual({ prefix: 4, suffix: 4 });
    expect(getDefaultTruncation('contract')).toEqual({ prefix: 6, suffix: 4 });
    expect(getDefaultTruncation('asset')).toEqual({ prefix: 6, suffix: 4 });
    expect(getDefaultTruncation('pool')).toEqual({ prefix: 6, suffix: 4 });
    expect(getDefaultTruncation('nft')).toEqual({ prefix: 6, suffix: 4 });
  });

  it('uses the no-truncate (0, 0) config for ledger entities (numeric, not hash)', () => {
    expect(getDefaultTruncation('ledger')).toEqual({ prefix: 0, suffix: 0 });
  });
});
