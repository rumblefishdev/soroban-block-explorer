import type { PoolAssetLeg, PoolItem } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';

import { PoolKpiStrip } from '../PoolKpiStrip.js';

function leg(asset_code: string, reserve: string | null): PoolAssetLeg {
  return {
    asset_code,
    asset_type_name: 'classic_credit',
    contract_id: null,
    issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
    icon_url: null,
    symbol: null,
    reserve,
  };
}

function pool(overrides: Partial<PoolItem>): PoolItem {
  return {
    legs: [leg('USDC', '100'), leg('EURC', '200')],
    total_shares: '150',
    participant_count: 3,
    latest_snapshot_ledger: null,
    latest_snapshot_at: null,
    ...overrides,
  } as PoolItem;
}

describe('PoolKpiStrip captions', () => {
  it('captions a present value by what it is', () => {
    renderWithProviders(<PoolKpiStrip pool={pool({})} />);
    expect(screen.queryByText('no recent snapshot')).toBeNull();
    expect(screen.getByText('shares outstanding')).toBeInTheDocument();
  });

  it('says "not indexed" for a missing value', () => {
    renderWithProviders(
      <PoolKpiStrip
        pool={pool({
          total_shares: null,
          legs: [leg('USDC', null), leg('EURC', '1')],
        })}
      />
    );
    expect(screen.getAllByText('not indexed')).toHaveLength(2);
    expect(screen.queryByText('no recent snapshot')).toBeNull();
  });

  // A classic pool writes a snapshot on every change, so an old snapshot is
  // a quiet pool's current state — its values are shown plainly.
  it('shows an old classic snapshot without a staleness caption', () => {
    renderWithProviders(
      <PoolKpiStrip
        pool={pool({
          latest_snapshot_ledger: 50_000_000,
          latest_snapshot_at: '2024-01-01T00:00:00Z',
        })}
      />
    );
    expect(screen.queryByText('no recent snapshot')).toBeNull();
    expect(screen.getByText('shares outstanding')).toBeInTheDocument();
  });
});
