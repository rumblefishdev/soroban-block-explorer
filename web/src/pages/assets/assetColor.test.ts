import { describe, expect, it } from 'vitest';

import { assetColor } from './assetColor.js';

describe('assetColor', () => {
  it('is deterministic — same code always yields the same colour', () => {
    expect(assetColor('USDC')).toEqual(assetColor('USDC'));
  });

  it('is case-insensitive on the code', () => {
    expect(assetColor('usdc')).toEqual(assetColor('USDC'));
  });

  it('returns a well-formed colour triplet', () => {
    const c = assetColor('XLM');
    expect(typeof c.bg).toBe('string');
    expect(typeof c.fg).toBe('string');
    expect(typeof c.dot).toBe('string');
  });
});
