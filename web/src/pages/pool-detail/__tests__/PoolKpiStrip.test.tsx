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
  // A Soroban pool has no snapshots at all. Keying the caption off snapshot
  // freshness told every one of them "no recent snapshot" beside a current
  // value — the caption follows the value instead.
  it('never stale-captions a value that is present', () => {
    renderWithProviders(<PoolKpiStrip pool={pool({})} />);
    expect(screen.queryByText('no recent snapshot')).toBeNull();
    expect(screen.getByText('shares outstanding')).toBeInTheDocument();
  });

  it('says "not indexed" for a missing value on a pool with no snapshots', () => {
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

  it('says "no recent snapshot" for a missing value on a stale classic pool', () => {
    renderWithProviders(
      <PoolKpiStrip
        pool={pool({
          total_shares: null,
          latest_snapshot_ledger: 50_000_000,
          latest_snapshot_at: '2024-01-01T00:00:00Z',
        })}
      />
    );
    expect(screen.getByText('no recent snapshot')).toBeInTheDocument();
    expect(screen.queryByText('not indexed')).toBeNull();
  });
});
