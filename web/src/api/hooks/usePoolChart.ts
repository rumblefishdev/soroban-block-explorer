import { getPoolChartOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';

import { detailPolicy } from '../polling.js';

/**
 * Chart period presets per Figma (node `325:24354`). The values are the
 * keys produced by the `TimeSeriesChart` interval selector (task 0065).
 */
export type ChartPeriod = '1D' | '7D' | '30D' | '1Y';

const HOUR_MS = 60 * 60 * 1000;
const DAY_MS = 24 * HOUR_MS;

/**
 * Per-preset backend params + the bucket width that interval produces.
 * One table so the query params and the bucket-end stamping in
 * `PoolCharts` cannot drift apart.
 */
const PERIOD_CONFIG: Record<
  ChartPeriod,
  { interval: '1h' | '1d' | '1w'; spanMs: number; bucketMs: number }
> = {
  '1D': { interval: '1h', spanMs: DAY_MS, bucketMs: HOUR_MS },
  '7D': { interval: '1h', spanMs: 7 * DAY_MS, bucketMs: HOUR_MS },
  '30D': { interval: '1d', spanMs: 30 * DAY_MS, bucketMs: DAY_MS },
  '1Y': { interval: '1w', spanMs: 365 * DAY_MS, bucketMs: 7 * DAY_MS },
};

/** Width in ms of the buckets the backend returns for a period preset. */
export const periodBucketMs = (period: ChartPeriod): number =>
  PERIOD_CONFIG[period].bucketMs;

/**
 * Maps a period preset to the underlying backend `(interval, from)`
 * params. `to` is omitted — the backend defaults it to `now()`.
 */
function periodToQueryParams(period: ChartPeriod): {
  interval: '1h' | '1d' | '1w';
  from: string;
} {
  const { interval, spanMs } = PERIOD_CONFIG[period];
  return { interval, from: new Date(Date.now() - spanMs).toISOString() };
}

/**
 * Fetches the time-series chart for a single liquidity pool
 * (`GET /liquidity-pools/:id/chart`). The `period` arg selects one of the
 * four Figma presets (1D / 7D / 30D / 1Y) and is translated to the
 * backend's `(interval, from)` params.
 *
 * Disabled until both a pool id and a period are present.
 */
export const usePoolChart = (poolId: string, period: ChartPeriod) => {
  // `from` is anchored at the first render that produced this period —
  // the memo only re-runs when `period` changes, so a long-lived session
  // on the same preset shows data starting at "X ago from the time the
  // page loaded", not literal "X ago from now". Acceptable for an
  // explorer (5-min stale-time triggers a refetch which recomputes
  // `from` on memo re-evaluation after remount); a trading UI would
  // want a periodic re-anchor.
  const query = useMemo(() => periodToQueryParams(period), [period]);
  return useQuery({
    ...getPoolChartOptions({
      path: { pool_id: poolId },
      query,
    }),
    ...detailPolicy,
    enabled: poolId.length > 0,
  });
};
