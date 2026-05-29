import { describe, expect, it } from 'vitest';

import { ASSET_TYPE_FILTERS, assetTypeMeta, iconKindFor } from './assetType.js';

describe('assetTypeMeta', () => {
  it('maps known type names to label + color', () => {
    expect(assetTypeMeta('native')).toEqual({ label: 'Native', color: 'blue' });
    expect(assetTypeMeta('classic_credit')).toEqual({
      label: 'Classic',
      color: 'neutral',
    });
    expect(assetTypeMeta('sac')).toEqual({ label: 'SAC', color: 'brown' });
    expect(assetTypeMeta('soroban')).toEqual({
      label: 'Soroban',
      color: 'emerald',
    });
  });

  it('falls back to the raw type as label with neutral color when unknown', () => {
    expect(assetTypeMeta('mystery')).toEqual({
      label: 'mystery',
      color: 'neutral',
    });
  });

  it('falls back to "Unknown" when type is null/undefined', () => {
    expect(assetTypeMeta(null)).toEqual({ label: 'Unknown', color: 'neutral' });
    expect(assetTypeMeta(undefined)).toEqual({
      label: 'Unknown',
      color: 'neutral',
    });
  });

  it('renders empty type name as-is (nullish coalescing keeps "")', () => {
    expect(assetTypeMeta('')).toEqual({ label: '', color: 'neutral' });
  });
});

describe('iconKindFor', () => {
  it('maps each known type name to the avatar kind', () => {
    expect(iconKindFor('native')).toBe('native');
    expect(iconKindFor('classic_credit')).toBe('classic');
    expect(iconKindFor('sac')).toBe('sac');
    // Soroban tokens reuse the classic avatar palette today.
    expect(iconKindFor('soroban')).toBe('classic');
  });

  it('falls back to the default kind for unknown / null / undefined', () => {
    expect(iconKindFor(null)).toBe('default');
    expect(iconKindFor(undefined)).toBe('default');
    expect(iconKindFor('mystery')).toBe('default');
  });
});

describe('ASSET_TYPE_FILTERS', () => {
  it('exposes "All types" plus the user-pickable subtypes', () => {
    expect(ASSET_TYPE_FILTERS.map((f) => f.value)).toEqual([
      '',
      'classic_credit',
      'sac',
      'soroban',
    ]);
    expect(ASSET_TYPE_FILTERS[0].label).toBe('All types');
  });
});
