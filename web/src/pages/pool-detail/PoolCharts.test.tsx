import type { ChartDataPoint } from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import { toChartPoints } from './PoolCharts.js';

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
