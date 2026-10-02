import { describe, expect, it } from 'vitest';

import { networkFrom } from '../network.js';

describe('networkFrom', () => {
  it('reads mainnet when the build sets nothing, as production always has', () => {
    expect(networkFrom(undefined)).toBe('mainnet');
    expect(networkFrom('')).toBe('mainnet');
    expect(networkFrom('mainnet')).toBe('mainnet');
  });

  it('reads testnet', () => {
    expect(networkFrom('testnet')).toBe('testnet');
  });

  it('refuses a misspelt name instead of showing mainnet', () => {
    expect(() => networkFrom('tesnet')).toThrow(/tesnet/);
    expect(() => networkFrom('Testnet')).toThrow();
  });
});
