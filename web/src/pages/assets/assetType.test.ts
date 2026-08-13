import { describe, expect, it } from 'vitest';

import { ASSET_TYPE_FILTERS, assetTypeMeta, SAC_TAG } from './assetType.js';

describe('assetTypeMeta', () => {
  it('maps known type names to label + color', () => {
    expect(assetTypeMeta('native')).toEqual({ label: 'Native', color: 'blue' });
    expect(assetTypeMeta('classic_credit')).toEqual({
      label: 'Classic credit',
      color: 'neutral',
    });
    expect(assetTypeMeta('soroban')).toEqual({
      label: 'Soroban',
      color: 'emerald',
    });
  });

  it('does not treat "sac" as a type — SAC is a separate property tag (ADR 0051)', () => {
    // `sac` is no longer an `asset_type_name`; it falls back to the raw label.
    expect(assetTypeMeta('sac')).toEqual({ label: 'sac', color: 'neutral' });
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

describe('SAC_TAG', () => {
  it('is the standalone SAC property tag (orthogonal to the type badge)', () => {
    expect(SAC_TAG).toEqual({ label: 'SAC', color: 'brown' });
  });
});

describe('ASSET_TYPE_FILTERS', () => {
  it('exposes "All types" plus the pickable asset types (no SAC — a property filter)', () => {
    expect(ASSET_TYPE_FILTERS.map((f) => f.value)).toEqual([
      '',
      'classic_credit',
      'soroban',
    ]);
    expect(ASSET_TYPE_FILTERS[0].label).toBe('All types');
  });
});
