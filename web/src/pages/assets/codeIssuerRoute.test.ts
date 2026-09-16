import { describe, expect, it } from 'vitest';

import { codeIssuerRoute } from './codeIssuerRoute.js';

// USDT0's real issuer — the asset task 0534 was reproduced on.
const ISSUER = 'GATISXX6BZ6NC7IKQBY37CJD4SOZL3CYZJWXEDG6JVIY4WBS6KXJHN6Q';
const ASSET_PAGE = `/assets/USDT0-${ISSUER}`;

describe('codeIssuerRoute', () => {
  it('routes both separators to the asset page', () => {
    expect(codeIssuerRoute(`USDT0:${ISSUER}`)).toBe(ASSET_PAGE);
    expect(codeIssuerRoute(`USDT0-${ISSUER}`)).toBe(ASSET_PAGE);
  });

  it('trims a pasted value', () => {
    expect(codeIssuerRoute(`  USDT0:${ISSUER}\n`)).toBe(ASSET_PAGE);
  });

  it('upper-cases the issuer but keeps the code case', () => {
    expect(codeIssuerRoute(`uSdT0:${ISSUER.toLowerCase()}`)).toBe(
      `/assets/uSdT0-${ISSUER}`
    );
  });

  it('leaves anything that is not a pair to the code filter', () => {
    for (const q of [
      '',
      'USDT0',
      'native',
      ISSUER,
      'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA',
      `:${ISSUER}`, // no code
      'USDT0:', // no issuer
      `USDT0:${ISSUER.slice(0, 55)}`, // issuer one character short
      `TOOLONGCODE13:${ISSUER}`, // code over alphanum12
      `US$T:${ISSUER}`, // code not alphanumeric
    ]) {
      expect(codeIssuerRoute(q), q).toBeNull();
    }
  });
});
