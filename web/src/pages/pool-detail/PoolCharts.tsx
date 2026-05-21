import { Box, Stack, Typography } from '@mui/material';
import {
  CardSkeleton,
  LazySection,
  Tabs,
  TimeSeriesChart,
  type TimeSeriesPoint,
} from '@rumblefish/soroban-block-explorer-ui';
import { useMemo, useState } from 'react';

import { usePoolChart, type ChartPeriod } from '../../api/index.js';
import { SectionCard } from '../detail/SectionCard.js';

type ChartMetric = 'tvl' | 'volume' | 'fees';

const TABS = [
  { key: 'tvl', label: 'TVL' },
  { key: 'volume', label: 'Volume' },
  { key: 'fees', label: 'Fees' },
] as const;

const PERIODS: ChartPeriod[] = ['1D', '7D', '30D', '1Y'];

/**
 * Module-level — Intl.NumberFormat construction is not free, and the
 * chart calls `valueFormatter` once per axis tick and once per tooltip
 * hover, so per-call instantiation adds up.
 */
const USD_COMPACT_FORMATTER = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  notation: 'compact',
  maximumFractionDigits: 1,
});

/** Currency formatter for chart y-axis + tooltip — values are USD amounts. */
const usdFormatter = (value: number): string =>
  USD_COMPACT_FORMATTER.format(value);

interface PoolChartsProps {
  poolId: string;
}

function PoolChartsContent({ poolId }: { poolId: string }) {
  const [metric, setMetric] = useState<ChartMetric>('tvl');
  const [period, setPeriod] = useState<ChartPeriod>('30D');

  const { data, isLoading, isError } = usePoolChart(poolId, period);

  /**
   * Map the API's `(bucket, tvl|volume|fee_revenue)` rows into the
   * `TimeSeriesPoint[]` shape consumed by `TimeSeriesChart`. Skip rows
   * where the chosen metric is null so the chart doesn't render gaps as
   * zero values.
   */
  const { points, allNull } = useMemo(() => {
    if (!data) return { points: [], allNull: false };
    const field: 'tvl' | 'volume' | 'fee_revenue' =
      metric === 'fees' ? 'fee_revenue' : metric;
    let nonNullSeen = false;
    const pts: TimeSeriesPoint[] = [];
    for (const row of data.data_points) {
      const raw = row[field];
      if (raw == null) continue;
      const num = Number(raw);
      if (!Number.isFinite(num)) continue;
      nonNullSeen = true;
      pts.push({ timestamp: row.bucket, value: num });
    }
    return { points: pts, allNull: !nonNullSeen };
  }, [data, metric]);

  // Series-null overlay per 0215 §6.14. The placeholder is intentional UX
  // until task 0199 (LP analytics, blocked-on-oracle) ships; 0250 removes
  // it. We still render the chart structure (tabs, period selector) above.
  const showPendingOraclePlaceholder = !isLoading && !isError && allNull;

  return (
    <SectionCard
      title="Activity"
      action={
        <Tabs
          tabs={[...TABS]}
          activeKey={metric}
          onChange={(key) => setMetric(key as ChartMetric)}
          aria-label="Chart metric"
        />
      }
    >
      {showPendingOraclePlaceholder ? (
        <Box sx={{ p: 4, textAlign: 'center' }}>
          <Typography variant="bodyRegular" color="text.secondary">
            Chart data not yet available
          </Typography>
          <Typography variant="bodySmRegular" color="text.tertiary">
            Pending the price-oracle integration (task 0199).
          </Typography>
        </Box>
      ) : (
        <Box sx={{ p: 2 }}>
          <TimeSeriesChart
            data={points}
            variant="area"
            valueFormatter={usdFormatter}
            intervals={PERIODS.map((p) => ({ key: p, label: p }))}
            activeInterval={period}
            onIntervalChange={(key) => setPeriod(key as ChartPeriod)}
            loading={isLoading}
            emptyState={
              <Stack
                spacing={0.5}
                alignItems="center"
                sx={{ py: 4, textAlign: 'center' }}
              >
                <Typography variant="bodyRegular" color="text.secondary">
                  No activity in this period
                </Typography>
                <Typography variant="bodySmRegular" color="text.tertiary">
                  Try a longer range.
                </Typography>
              </Stack>
            }
          />
        </Box>
      )}
    </SectionCard>
  );
}

/**
 * Chart card on the LP detail page (Figma node `325:24354`). One chart
 * with three metric tabs (TVL / Volume / Fees) and a four-preset range
 * selector (1D / 7D / 30D / 1Y). Lazy-fetched: the chart endpoint is
 * only hit when the section scrolls into view.
 *
 * All three metric series come back null until task 0199 ships the
 * price-oracle integration — in that case we render an inline
 * "Chart data not yet available" placeholder per 0215 §6.14 rather than
 * empty axes. The shape of the placeholder is removed by 0250 once 0199
 * lands.
 */
export function PoolCharts({ poolId }: PoolChartsProps) {
  return (
    <LazySection
      placeholder={<CardSkeleton />}
      minHeight={420}
      rootMargin="200px"
    >
      <PoolChartsContent poolId={poolId} />
    </LazySection>
  );
}
