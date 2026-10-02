import type { ChartDataPoint } from '@rumblefish/api-types';
import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../test-utils.js';
import { PoolCharts, toChartPoints } from '../PoolCharts.js';

const hookMock = vi.hoisted(() => ({ usePoolChart: vi.fn() }));

vi.mock('../../../api/index.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../api/index.js')>()),
  usePoolChart: hookMock.usePoolChart,
}));

const WEEK_MS = 7 * 24 * 60 * 60 * 1000;

const row = (bucket: string, tvl: string | null): ChartDataPoint => ({
  bucket,
  tvl,
  samples_in_bucket: 1,
});

describe('toChartPoints', () => {
  it('stamps a state metric at the bucket END, not its start', () => {
    // Weekly bucket Mon Aug 3 whose argMax TVL was measured near Aug 9 —
    // the point belongs at the week's end, else stepAfter draws the next
    // week's drop from Monday (the 1Y "cliff a week early" bug).
    const pts = toChartPoints(
      [row('2026-08-03T00:00:00Z', '40492.13')],
      'tvl',
      WEEK_MS,
      Date.parse('2026-08-18T12:00:00Z')
    );
    expect(pts).toEqual([
      { timestamp: Date.parse('2026-08-10T00:00:00Z'), value: 40492.13 },
    ]);
  });

  it('clamps the in-progress bucket to now', () => {
    const now = Date.parse('2026-08-18T09:42:40Z');
    const pts = toChartPoints(
      [row('2026-08-17T00:00:00Z', '9934.41')],
      'tvl',
      WEEK_MS,
      now
    );
    expect(pts[0]?.timestamp).toBe(now);
  });

  it('keeps flows (shift 0) on the bucket start', () => {
    const pts = toChartPoints(
      [
        {
          bucket: '2026-08-10T00:00:00Z',
          volume: '12.5',
          samples_in_bucket: 3,
        },
      ],
      'volume',
      0,
      Date.parse('2026-08-18T12:00:00Z')
    );
    expect(pts).toEqual([
      { timestamp: Date.parse('2026-08-10T00:00:00Z'), value: 12.5 },
    ]);
  });

  it('drops null-metric rows instead of zeroing them', () => {
    const pts = toChartPoints(
      [row('2026-08-03T00:00:00Z', null), row('2026-08-10T00:00:00Z', '5')],
      'tvl',
      WEEK_MS,
      Date.parse('2026-08-18T12:00:00Z')
    );
    expect(pts).toHaveLength(1);
    expect(pts[0]?.value).toBe(5);
  });
});

describe('PoolCharts', () => {
  const chart = {
    data: {
      data_points: [
        {
          bucket: '2026-09-28T00:00:00Z',
          tvl: '8026654.16',
          samples_in_bucket: 3,
        },
      ],
    },
    isLoading: false,
    isError: false,
    error: null,
    refetch: vi.fn(),
  };

  const twoLegs = {
    legs: [{}, {}],
    pool_kind: 'soroban',
    volume_priceable: true,
  } as Parameters<typeof PoolCharts>[0]['pool'];

  /** A two-leg pool with no trades in the window has no activity to plot,
   *  soroban or classic alike. */
  it('says no activity for a priceable pool with no trades', () => {
    hookMock.usePoolChart.mockReturnValue(chart);
    renderWithProviders(<PoolCharts poolId="CDMH" pool={twoLegs} />);

    fireEvent.click(screen.getByRole('tab', { name: 'Volume' }));

    expect(screen.queryByText('Not indexed yet')).toBeNull();
    expect(screen.getByText('No activity in this period')).toBeInTheDocument();
  });

  /** A three-leg pool trades, but its volume is never priced: the tab must
   *  say so, not claim the pool was idle (W211). */
  it('says volume is not priced for a pool the API cannot price', () => {
    hookMock.usePoolChart.mockReturnValue(chart);
    const threeLegs = {
      ...twoLegs,
      legs: [{}, {}, {}],
      volume_priceable: false,
    };
    renderWithProviders(<PoolCharts poolId="CJAQ" pool={threeLegs} />);

    fireEvent.click(screen.getByRole('tab', { name: 'Volume' }));

    expect(screen.getByText('Volume not priced')).toBeInTheDocument();
    expect(screen.getByText(/this pool has 3/)).toBeInTheDocument();
    expect(screen.queryByText('No activity in this period')).toBeNull();
  });
});
