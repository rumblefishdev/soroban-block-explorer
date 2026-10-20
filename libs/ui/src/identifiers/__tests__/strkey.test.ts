import { describe, expect, it } from 'vitest';

import { unmuxed } from '../strkey.js';

// Real keys and their muxed forms, as stellar-strkey 1.0 encodes them:
// an account with sub-account id 3539365402, a contract with id 42
// (CAP-0084).
const REAL_G = 'GD5SL5RIC5STHGDDOJGSIIZHZZQPA4HIYFEPIQ3FF7M3H2F5VQ7K2FTL';
const REAL_M =
  'MD5SL5RIC5STHGDDOJGSIIZHZZQPA4HIYFEPIQ3FF7M3H2F5VQ7K2AAAAAANF5TODJEYA';
const REAL_C = 'CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA';
const REAL_W =
  'WDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIAAAAAAAAAAAFLR56';

describe('unmuxed', () => {
  it('names the account an M-strkey and the contract a W-strkey stands for', () => {
    expect(unmuxed(REAL_M)).toBe(REAL_G);
    expect(unmuxed(REAL_W)).toBe(REAL_C);
  });

  it('is null for anything but a valid M- or W-strkey', () => {
    expect(unmuxed(REAL_G)).toBeNull();
    expect(unmuxed(REAL_C)).toBeNull();
    expect(unmuxed('W')).toBeNull();
    expect(unmuxed(REAL_W.toLowerCase())).toBeNull();
    // One changed character breaks the checksum.
    const broken =
      REAL_W.slice(0, 10) + (REAL_W[10] === 'A' ? 'B' : 'A') + REAL_W.slice(11);
    expect(unmuxed(broken)).toBeNull();
    // Same bits but the padding bit set: not the canonical key, so the API
    // would refuse it — the UI must not link it either.
    const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
    const padded =
      REAL_W.slice(0, 68) + alphabet[alphabet.indexOf(REAL_W[68]) | 1];
    expect(padded).not.toBe(REAL_W);
    expect(unmuxed(padded)).toBeNull();
  });
});
