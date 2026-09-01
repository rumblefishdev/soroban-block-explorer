import type { PoolAssetLeg } from '@rumblefish/api-types';
import { formatCompactAmount } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it } from 'vitest';

import { assetLegLabel, legHref } from './helpers.js';

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
  // Changed in task 0472 — this case previously asserted `undefined`. Not a
  // regression: `/assets/native` became the canonical asset token in 0243,
  // which retired the "native has no address" rationale this rule was built
  // on. XLM was the only leg in the app that rendered as dead text.
  it('links native legs to the canonical /assets/native token', () => {
    expect(legHref(makeLeg({ asset_type: 0, asset_type_name: 'native' }))).toBe(
      '/assets/native'
    );
  });

  it('prefers the canonical native token over an XLM SAC mirror', () => {
    expect(
      legHref(
        makeLeg({
          asset_type: 0,
          asset_type_name: 'native',
          asset_code: null,
          issuer: null,
          contract_id:
            'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA',
        })
      )
    ).toBe('/assets/native');
  });

  // Flipped in task 0472 — this case previously asserted contract_id-first.
  // Intentional: task 0364 dropped SAC-facet aliasing from the assets
  // endpoint, so /assets/{SAC C…} 404s and the pair is the only live route
  // for a classic leg (~93k legs carry a SAC mirror on prod).
  it('prefers code-issuer over the SAC mirror, whose address now 404s', () => {
    expect(
      legHref(
        makeLeg({
          contract_id:
            'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC',
        })
      )
    ).toBe(
      `/assets/${encodeURIComponent(
        'USDC-GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN'
      )}`
    );
  });

  it('falls back to contract_id only when the pair is incomplete', () => {
    expect(
      legHref(
        makeLeg({
          asset_code: null,
          issuer: null,
          asset_type: 3,
          asset_type_name: 'soroban',
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

// ---------------------------------------------------------------------------
// Unified leg views (task 0374)
// ---------------------------------------------------------------------------

import type { PoolItem, PoolLegItem } from '@rumblefish/api-types';

import { legItemLabel, poolLegViews, poolLegsLabel } from './helpers.js';

function makeLegItem(overrides: Partial<PoolLegItem> = {}): PoolLegItem {
  return {
    family: 'soroban',
    asset_code: null,
    issuer: null,
    contract_id: 'CC5PU23MKXHUFJKGG5FAUG7MFZX2KMWXPNZP26DDYW76VCB26UWMPEI6',
    symbol: 'AQUA',
    name: 'Aquarius',
    decimals: 7,
    reserve: '250000000000',
    ...overrides,
  };
}

function makeSorobanPool(legs: PoolLegItem[]): PoolItem {
  return {
    pool_id: 'CDJ2WSFTWIINF4NGP4RIBHT5QSSTHOJ2LA6HN5ZI53CL23LI4MZTQNWY',
    pool_kind: 'soroban',
    protocol: 'aquarius',
    pool_type: 'constant',
    legs,
    asset_a: null,
    asset_b: null,
    fee_bps: 10,
    fee_percent: '0.1',
    created_at_ledger: 63893403,
    participant_count: null,
    latest_snapshot_ledger: null,
    reserve_a: null,
    reserve_b: null,
    total_shares: null,
    tvl: null,
    volume: null,
    fee_revenue: null,
    latest_snapshot_at: null,
  };
}

describe('legItemLabel', () => {
  it('follows the precedence native → code → symbol → truncated contract', () => {
    expect(legItemLabel(makeLegItem({ family: 'native', symbol: null }))).toBe(
      'XLM'
    );
    expect(
      legItemLabel(
        makeLegItem({ family: 'classic_credit', asset_code: 'USDC' })
      )
    ).toBe('USDC');
    expect(legItemLabel(makeLegItem())).toBe('AQUA');
    expect(legItemLabel(makeLegItem({ symbol: null }))).toBe('CC5P…PEI6');
  });

  it('renders an unresolved leg as an explicit question mark, never a guess', () => {
    expect(
      legItemLabel(
        makeLegItem({
          family: 'unresolved',
          symbol: null,
          contract_id: null,
        })
      )
    ).toBe('?');
  });
});

describe('poolLegViews (soroban)', () => {
  it('scales raw reserves by the leg decimals — exactly, including 18dp', () => {
    const views = poolLegViews(
      makeSorobanPool([
        makeLegItem({ reserve: '4112908590', decimals: 7 }),
        makeLegItem({ reserve: '1', decimals: 18, symbol: 'BIG' }),
      ])
    );
    expect(views[0].reserve).toBe('411.290859');
    expect(views[1].reserve).toBe('0.000000000000000001');
  });

  it('null decimals → null reserve (unknown scale must not render raw units)', () => {
    const views = poolLegViews(
      makeSorobanPool([makeLegItem({ decimals: null })])
    );
    expect(views[0].reserve).toBeNull();
  });

  it('joins leg labels into the pair label, 3-leg pools included', () => {
    const pool = makeSorobanPool([
      makeLegItem({ family: 'native', symbol: null }),
      makeLegItem(),
      makeLegItem({ symbol: 'USDx' }),
    ]);
    expect(poolLegsLabel(pool)).toBe('XLM / AQUA / USDx');
  });
});
