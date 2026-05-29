import { describe, expect, it } from 'vitest';

import { directRouteFor } from './directRouteFor.js';

describe('directRouteFor', () => {
  it('returns the ledger route for a bare positive integer', () => {
    expect(directRouteFor('1')).toBe('/ledgers/1');
    expect(directRouteFor('54837201')).toBe('/ledgers/54837201');
  });

  it('trims surrounding whitespace before matching', () => {
    expect(directRouteFor('  42  ')).toBe('/ledgers/42');
  });

  it('returns null for zero (sequence numbering starts at 1)', () => {
    expect(directRouteFor('0')).toBeNull();
  });

  it('returns null for sequence numbers above u32 max', () => {
    // 0xFFFFFFFF is the cap; one above must not route.
    expect(directRouteFor('4294967296')).toBeNull();
  });

  it('returns null for non-digit inputs', () => {
    expect(directRouteFor('')).toBeNull();
    expect(directRouteFor('abc')).toBeNull();
    // Decimal isn't a valid u32 sequence — fall through.
    expect(directRouteFor('1.5')).toBeNull();
    expect(directRouteFor('-42')).toBeNull();
  });

  it('returns null for strkeys / hashes (handled by the backend search redirect)', () => {
    expect(
      directRouteFor('GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOUJ3K4MOMNGEE36')
    ).toBeNull();
    expect(
      directRouteFor(
        '7b2a8c1f9d4e6a3b8c1f9d4e6a3b8c1f9d4e6a3b8c1f9d4e6a3b8c1f9d4e6a3b'
      )
    ).toBeNull();
  });
});
