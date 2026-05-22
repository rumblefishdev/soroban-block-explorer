import { getPoolChartOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';

import { detailPolicy } from '../polling.js';

/**
 * Chart period presets per Figma (node `325:24354`). The values are the
 * keys produced by the `TimeSeriesChart` interval selector (task 0065).
 */
export type ChartPeriod = '1D' | '7D' | '30D' | '1Y';

const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * Maps a period preset to the underlying backend `(interval, from)`
 * params. `to` is omitted — the backend defaults it to `now()`.
 */
function periodToQueryParams(period: ChartPeriod): {
  interval: '1h' | '1d' | '1w';
  from: string;
} {
  const now = Date.now();
  switch (period) {
    case '1D':
      return { interval: '1h', from: new Date(now - DAY_MS).toISOString() };
    case '7D':
      return {
        interval: '1h',
        from: new Date(now - 7 * DAY_MS).toISOString(),
      };
    case '30D':
      return {
        interval: '1d',
        from: new Date(now - 30 * DAY_MS).toISOString(),
      };
    case '1Y':
      return {
        interval: '1w',
        from: new Date(now - 365 * DAY_MS).toISOString(),
      };
  }
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
