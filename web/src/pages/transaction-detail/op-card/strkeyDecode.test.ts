import { describe, expect, it } from 'vitest';

import { isValidStrkey } from './strkeyDecode.js';

// Real mainnet identifiers.
const ACCOUNT = 'GC4QMEH5CY5HAEZVC2XNTRV2XBPQWUX2WCV3ANU32HBFNCYIKWHGK7XQ';
const CONTRACT = 'CDDTJ7OZU2WZAEZNTUZWIRAE4EMP5CF63M3INFQWTLX4ENMYUFK6RCTX';
const POOL = 'LBSUSB6VB2DTUIXNO5QLKUKSKRKVL4SPDS354FTY7O45JT2E72VHYSTD';

describe('isValidStrkey', () => {
  it('accepts real account, contract and pool ids', () => {
    for (const value of [ACCOUNT, CONTRACT, POOL]) {
      expect(isValidStrkey(value)).toBe(true);
    }
  });

  it('rejects a shape-perfect value whose checksum is wrong', () => {
    // Flip one payload character: still 56 base32 chars starting with G, so
    // the shape regex is satisfied and only the CRC can tell.
    const tampered = `${ACCOUNT.slice(0, 10)}${
      ACCOUNT[10] === 'A' ? 'B' : 'A'
    }${ACCOUNT.slice(11)}`;
    expect(tampered).toHaveLength(56);
    expect(/^[GCL][A-Z2-7]{55}$/.test(tampered)).toBe(true);
    expect(isValidStrkey(tampered)).toBe(false);
  });

  it('rejects a version byte that contradicts the leading letter', () => {
    // A contract id relabelled as an account keeps the shape and the CRC of
    // its own payload, but the version byte no longer matches `G`.
    const relabelled = `G${CONTRACT.slice(1)}`;
    expect(isValidStrkey(relabelled)).toBe(false);
  });

  it('rejects wrong length, wrong prefix and non-base32 characters', () => {
    expect(isValidStrkey(ACCOUNT.slice(0, 55))).toBe(false);
    expect(isValidStrkey(`M${ACCOUNT.slice(1)}`)).toBe(false);
    expect(isValidStrkey(`${ACCOUNT.slice(0, 55)}1`)).toBe(false);
  });
});
