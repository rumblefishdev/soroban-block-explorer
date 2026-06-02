import type { PoolAssetLeg } from '@rumblefish/api-types';
import { formatCompactAmount } from '@rumblefish/soroban-block-explorer-ui';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { assetLegLabel, isPoolStale, legHref } from './helpers.js';

function makeLeg(overrides: Partial<PoolAssetLeg> = {}): PoolAssetLeg {
  return {
    asset_code: 'USDC',
    asset_type: 1,
    asset_type_name: 'classic_credit',
    contract_id: null,
    issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
    ...overrides,
  };
}

describe('legHref', () => {
  it('returns undefined for native legs (no on-chain address)', () => {
    expect(
      legHref(makeLeg({ asset_type: 0, asset_type_name: 'native' }))
    ).toBeUndefined();
  });

  it('prefers contract_id (SAC mirror) over code/issuer', () => {
    expect(
      legHref(
        makeLeg({
          contract_id:
            'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC',
        })
      )
    ).toBe('/assets/CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC');
  });

  it('falls back to /assets/{code}-{issuer} for classic credit legs', () => {
    const href = legHref(makeLeg());
    expect(href).toBe(
      `/assets/${encodeURIComponent(
        'USDC-GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN'
      )}`
    );
  });

  it('returns undefined for schema-drift legs (no code, no contract id)', () => {
    expect(
      legHref(makeLeg({ asset_code: null, issuer: null, contract_id: null }))
    ).toBeUndefined();
  });
});

describe('assetLegLabel', () => {
  it('returns "XLM" for the native leg', () => {
    expect(
      assetLegLabel(
        makeLeg({ asset_type: 0, asset_type_name: 'native', asset_code: null })
      )
    ).toBe('XLM');
  });

  it('returns the asset_code for non-native legs', () => {
    expect(assetLegLabel(makeLeg({ asset_code: 'USDC' }))).toBe('USDC');
    expect(assetLegLabel(makeLeg({ asset_code: 'EURC' }))).toBe('EURC');
  });

  it('throws on schema drift (non-native leg with no asset_code)', () => {
    expect(() =>
      assetLegLabel(
        makeLeg({ asset_code: null, asset_type_name: 'classic_credit' })
      )
    ).toThrow(/no asset_code/);
  });
});

describe('isPoolStale', () => {
  beforeEach(() => {
    // Pin "now" so the freshness math is deterministic.
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-05-29T12:00:00Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns true when no snapshot timestamp is provided', () => {
    expect(isPoolStale(null)).toBe(true);
    expect(isPoolStale(undefined)).toBe(true);
    expect(isPoolStale('')).toBe(true);
  });

  it('returns false within the 7-day freshness window', () => {
    expect(isPoolStale('2026-05-29T11:00:00Z')).toBe(false); // 1h ago
    expect(isPoolStale('2026-05-23T13:00:00Z')).toBe(false); // ~6 days ago
  });

  it('returns true once the snapshot is older than 7 days', () => {
    expect(isPoolStale('2026-05-22T11:59:00Z')).toBe(true);
    expect(isPoolStale('2026-05-01T00:00:00Z')).toBe(true);
  });

  it('returns true on unparseable timestamps', () => {
    expect(isPoolStale('not a date')).toBe(true);
  });
});

describe('formatCompactAmount', () => {
  it('returns em-dash for null, undefined, and non-numeric input', () => {
    expect(formatCompactAmount(null)).toBe('—');
    expect(formatCompactAmount(undefined)).toBe('—');
    expect(formatCompactAmount('not a number')).toBe('—');
  });

  it('formats small numbers without notation', () => {
    expect(formatCompactAmount(0)).toBe('0');
    expect(formatCompactAmount(42)).toBe('42');
  });

  it('uses compact notation for larger numbers', () => {
    expect(formatCompactAmount(1_500)).toBe('1.5K');
    expect(formatCompactAmount(1_200_000)).toBe('1.2M');
    expect(formatCompactAmount('753982100.00')).toBe('754M');
  });
});
