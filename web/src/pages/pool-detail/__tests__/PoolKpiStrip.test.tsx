import type { PoolAssetLeg, PoolItem } from '@rumblefish/api-types';
import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';

import { PoolKpiStrip } from '../PoolKpiStrip.js';

function leg(
  asset_code: string,
  reserve: string | null,
  decimals: number | null = 7
): PoolAssetLeg {
  return {
    asset_code,
    asset_type_name: 'classic_credit',
    contract_id: null,
    issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
    icon_url: null,
    symbol: null,
    reserve,
    decimals,
  };
}

function pool(overrides: Partial<PoolItem>): PoolItem {
  return {
    // RAW integers, scaled by `decimals` (7) on the client.
    legs: [leg('USDC', '1000000000'), leg('EURC', '2000000000')],
    total_shares: '1500000000',
    total_shares_decimals: 7,
    participant_count: 3,
    latest_snapshot_ledger: null,
    latest_snapshot_at: null,
    ...overrides,
  } as PoolItem;
}

describe('PoolKpiStrip captions', () => {
  it('captions a present value by what it is', () => {
    renderWithProviders(<PoolKpiStrip pool={pool({})} />);
    expect(screen.getByText('shares outstanding')).toBeInTheDocument();
  });

  it('says "not indexed" for a missing value', () => {
    renderWithProviders(
      <PoolKpiStrip
        pool={pool({
          total_shares: null,
          legs: [leg('USDC', null), leg('EURC', '10000000')],
        })}
      />
    );
    expect(screen.getAllByText('not indexed')).toHaveLength(2);
  });

  // A raw reserve with no known scale has no number to show: a guessed 7 is
  // 10^11 off for an 18-decimal token.
  it('says "not indexed" for a reserve whose scale is unknown', () => {
    renderWithProviders(
      <PoolKpiStrip
        pool={pool({
          legs: [leg('USDC', '1000000000'), leg('USST', '10000000000', null)],
        })}
      />
    );
    expect(screen.getByText('not indexed')).toBeInTheDocument();
  });

  // A Soroban pool's holders are not counted yet: unknown, never "0".
  it('captions an uncounted participant total as not indexed', () => {
    renderWithProviders(
      <PoolKpiStrip pool={pool({ participant_count: null })} />
    );
    expect(screen.getByText('not indexed')).toBeInTheDocument();
    expect(screen.queryByText('liquidity providers')).toBeNull();
  });
});
