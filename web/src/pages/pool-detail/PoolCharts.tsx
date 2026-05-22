import { Box, Card, Stack, Typography } from '@mui/material';
import {
  CardSkeleton,
  LazySection,
  Tabs,
  TimeSeriesChart,
  type TimeSeriesPoint,
} from '@rumblefish/soroban-block-explorer-ui';
import { useMemo, useState } from 'react';

import { usePoolChart, type ChartPeriod } from '../../api/index.js';

type ChartMetric = 'tvl' | 'volume' | 'fees';

// Figma uses the placeholder labels "Invocations" and "Events" alongside
// TVL — those are designer mock-ups (LPs do not have invocation or
// event streams). We keep TVL / Volume / Fees because those are the
// metrics the backend actually serves. The task notes flag this as an
// explicit deviation from Figma.
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

const USD_FULL_FORMATTER = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  maximumFractionDigits: 0,
});

const DATE_FORMATTER = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  year: 'numeric',
});

/** Currency formatter for chart y-axis + tooltip — values are USD amounts. */
const usdFormatter = (value: number): string =>
  USD_COMPACT_FORMATTER.format(value);

const METRIC_LABELS: Record<ChartMetric, string> = {
  tvl: 'TVL',
  volume: 'Volume',
  fees: 'Fees',
};

/**
 * Range-preset pill row (1D / 7D / 30D / 1Y). Active key is filled
 * with the brand yellow; the rest sit on a muted gray. Mirrors the
 * Figma chart-card top-right control (node `325:22339`).
 *
 * Mirrors the unexported `IntervalSelector` in
 * `libs/ui/src/visualization/TimeSeriesChart.tsx` — kept local rather
 * than touching the shared lib for this one page.
 */
function IntervalPills({
  intervals,
  active,
  onChange,
}: {
  intervals: readonly ChartPeriod[];
  active: ChartPeriod;
  onChange: (next: ChartPeriod) => void;
}) {
  return (
    <Stack direction="row" spacing={0.5} role="group" aria-label="Chart range">
      {intervals.map((key) => {
        const isActive = key === active;
        return (
          <Box
            key={key}
            component="button"
            type="button"
            aria-pressed={isActive}
            onClick={() => onChange(key)}
            sx={(theme) => ({
              ...theme.typography.bodyXsMedium,
              border: 'none',
              borderRadius: `${theme.shape.radius.s}px`,
              px: 1.25,
              py: 0.5,
              cursor: 'pointer',
              backgroundColor: isActive
                ? theme.palette.surface.primaryMain
                : theme.palette.surface.grayLight,
              color: isActive
                ? theme.palette.grey[900]
                : theme.palette.text.secondary,
              '&:hover': {
                backgroundColor: isActive
                  ? theme.palette.surface.primaryHover
                  : theme.palette.surface.grayHover,
              },
              '&:focus-visible': {
                outline: `2px solid ${theme.palette.stroke.action}`,
                outlineOffset: 2,
              },
            })}
          >
            {key}
          </Box>
        );
      })}
    </Stack>
  );
}

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
  // it. Tabs and range pills stay live so the structure reads as the
  // Figma chart card even when the chart body is empty.
  const showPendingOraclePlaceholder = !isLoading && !isError && allNull;

  // Latest point drives the chart heading (Figma `325:22339` headline
  // shows the last value + its bucket date).
  const latest =
    points.length > 0 ? points[points.length - 1] ?? undefined : undefined;
  const headline = latest ? USD_FULL_FORMATTER.format(latest.value) : '—';
  const headlineCaption = latest
    ? `${DATE_FORMATTER.format(new Date(latest.timestamp))} · ${
        METRIC_LABELS[metric]
      }`
    : METRIC_LABELS[metric];

  return (
    <Card sx={{ p: 0 }}>
      <Box
        sx={(theme) => ({
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          flexWrap: 'wrap',
          gap: 1,
          px: 2,
          py: 1,
          borderBottom: `1px solid ${theme.palette.stroke.default}`,
        })}
      >
        <Tabs
          tabs={[...TABS]}
          activeKey={metric}
          onChange={(key) => setMetric(key as ChartMetric)}
          aria-label="Chart metric"
        />
        <IntervalPills
          intervals={PERIODS}
          active={period}
          onChange={setPeriod}
        />
      </Box>

      <Box sx={{ p: 2 }}>
        {showPendingOraclePlaceholder ? (
          <Stack spacing={0.5} sx={{ py: 4, textAlign: 'center' }}>
            <Typography variant="bodyRegular" color="text.secondary">
              Chart data not yet available
            </Typography>
            <Typography variant="bodySmRegular" color="text.tertiary">
              Pending the price-oracle integration (task 0199).
            </Typography>
          </Stack>
        ) : (
          <TimeSeriesChart
            data={points}
            // Figma node `325:24354` shows a plain line (no area fill)
            // with hollow blue marks at each bucket. Switched away from
            // `area` so the chart reads as the design — fill obscures
            // the per-bucket dots on dark surface.
            variant="line"
            valueFormatter={usdFormatter}
            title={headline}
            subtitle={headlineCaption}
            // Range pills live in the card header above — suppress the
            // chart's built-in interval row so we don't get two copies.
            intervals={[]}
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
        )}
      </Box>
    </Card>
  );
}

/**
 * Chart card on the LP detail page (Figma node `325:22339`). One chart
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
