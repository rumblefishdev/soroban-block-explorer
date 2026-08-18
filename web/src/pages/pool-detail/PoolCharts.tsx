import { Box, Card, Stack, Typography } from '@mui/material';
import type { ChartDataPoint } from '@rumblefish/api-types';
import {
  CardSkeleton,
  LazySection,
  QueryErrorState,
  Tabs,
  TimeSeriesChart,
  type TimeSeriesCurve,
  type TimeSeriesPoint,
} from '@rumblefish/soroban-block-explorer-ui';
import { useMemo, useState } from 'react';

import {
  periodBucketMs,
  usePoolChart,
  type ChartPeriod,
} from '../../api/index.js';

type ChartMetric = 'tvl' | 'volume' | 'fees';

const TABS = [
  { key: 'tvl', label: 'TVL' },
  { key: 'fees', label: 'Fee revenue' },
  { key: 'volume', label: 'Volume' },
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

/**
 * Sub-dollar money needs significant digits, not fixed ones. Both
 * formatters above round to whole dollars or one compact digit, so a pool
 * whose TVL is $0.003 rendered every axis tick AND the headline as "$0" —
 * an axis carrying no information at all (seen on real pools; fee revenue
 * hits it constantly, since a 0.30% cut of a small trade is fractions of
 * a cent).
 */
const USD_SMALL_FORMATTER = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  maximumSignificantDigits: 2,
});

/**
 * Format a USD amount, switching to significant digits below a dollar.
 * Decided per VALUE rather than per chart, so a tick at $0.50 reads
 * "$0.50" even on an axis that runs to thousands. Exact zero keeps the
 * plain "$0" — it is zero, not an unreadably small number.
 */
function formatUsd(value: number, atOrAboveOne: Intl.NumberFormat): string {
  const abs = Math.abs(value);
  return abs > 0 && abs < 1
    ? USD_SMALL_FORMATTER.format(value)
    : atOrAboveOne.format(value);
}

const DATE_FORMATTER = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  year: 'numeric',
});

/** Currency formatter for chart y-axis + tooltip — values are USD amounts. */
const usdFormatter = (value: number): string =>
  formatUsd(value, USD_COMPACT_FORMATTER);

const METRIC_LABELS: Record<ChartMetric, string> = {
  tvl: 'TVL',
  volume: 'Volume',
  fees: 'Fee revenue',
};

/**
 * How each metric is drawn — decided by what the number IS, matching the
 * way the endpoint aggregates it (see the chart query, task 0199):
 *
 * - **TVL is a STATE** (`argMax` — the last value in the bucket). It only
 *   moves when a deposit / withdraw / swap lands and holds flat in
 *   between, so a step is the honest line. Smoothing drew gentle curves
 *   through values the pool never held.
 * - **Volume / fees are FLOWS** (`sum` over the bucket). Independent
 *   per-bucket totals with nothing in between to interpolate, so: bars.
 *   This also stops the series bridging over unpriceable buckets — those
 *   rows are dropped below, and a line joined straight across them (a
 *   provider price gap rendered as smooth drift).
 *
 * TVL is a bare line, NOT a filled area, and that was measured rather than
 * assumed: a fill is read by area, so it has to baseline at zero, and at
 * zero this pool's real 16% slide ($155 → $130) collapsed into a ripple on
 * top of a solid block. DefiLlama-style filled TVL works for protocols
 * whose TVL spans orders of magnitude — there the silhouette is the story.
 * Per-pool, the movement is the story, so the zoomed baseline a bare line
 * permits is worth more than the fill.
 */
const METRIC_RENDERING: Record<
  ChartMetric,
  { variant: 'line' | 'area' | 'bar'; curve?: TimeSeriesCurve }
> = {
  tvl: { variant: 'line', curve: 'stepAfter' },
  volume: { variant: 'bar' },
  fees: { variant: 'bar' },
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

/** Two-line placeholder for an empty chart body. */
function ChartEmptyState({ title, hint }: { title: string; hint: string }) {
  return (
    <Stack
      spacing={0.5}
      alignItems="center"
      sx={{ py: 4, textAlign: 'center', maxWidth: 420, mx: 'auto' }}
    >
      <Typography
        variant="bodyMedium"
        sx={(theme) => ({ color: theme.palette.text.secondary })}
      >
        {title}
      </Typography>
      <Typography
        variant="bodySmMedium"
        sx={(theme) => ({ color: theme.palette.text.tertiary })}
      >
        {hint}
      </Typography>
    </Stack>
  );
}

/**
 * Map API chart rows to chart points for one metric, dropping rows where
 * the metric is null or non-numeric (a gap must stay a gap, not become 0).
 *
 * `bucketEndShiftMs` moves each point from bucket START to bucket END,
 * clamped to `nowMs` for the in-progress bucket. TVL needs this: the
 * backend's per-bucket TVL is `argMax` — the state at the END of the
 * bucket — so stamped at the bucket start, `stepAfter` held an end-of-week
 * value from that week's Monday and drew a drop up to a bucket earlier
 * than it happened (task 0505: a real Aug 12–16 decline rendered as an
 * Aug 10 cliff on 1Y). Flows (volume / fees) pass 0 — a bar labeled by
 * its bucket start is "the week of Aug 10", which is right.
 */
export function toChartPoints(
  rows: readonly ChartDataPoint[],
  field: 'tvl' | 'volume' | 'fee_revenue',
  bucketEndShiftMs: number,
  nowMs: number
): TimeSeriesPoint[] {
  const pts: TimeSeriesPoint[] = [];
  for (const row of rows) {
    const raw = row[field];
    if (raw == null) continue;
    const num = Number(raw);
    if (!Number.isFinite(num)) continue;
    const bucketMs = new Date(row.bucket).getTime() + bucketEndShiftMs;
    pts.push({ timestamp: Math.min(bucketMs, nowMs), value: num });
  }
  return pts;
}

interface PoolChartsProps {
  poolId: string;
}

function PoolChartsContent({ poolId }: { poolId: string }) {
  const [metric, setMetric] = useState<ChartMetric>('tvl');
  const [period, setPeriod] = useState<ChartPeriod>('1D');

  const { data, isLoading, isError, error, refetch } = usePoolChart(
    poolId,
    period
  );

  // See `toChartPoints` — TVL (a state, argMax over the bucket) is stamped
  // at bucket END; volume / fees (flows) keep the bucket-start label.
  const points = useMemo(() => {
    if (!data) return [];
    const field: 'tvl' | 'volume' | 'fee_revenue' =
      metric === 'fees' ? 'fee_revenue' : metric;
    const shift = metric === 'tvl' ? periodBucketMs(period) : 0;
    return toChartPoints(data.data_points, field, shift, Date.now());
  }, [data, metric, period]);

  /**
   * Tell "nothing happened" apart from "we cannot price what happened".
   *
   * The endpoint emits a bucket row only where the pool had at least one
   * snapshot, so rows-but-no-plottable-points means the on-chain activity
   * IS there and only the USD conversion is missing. Saying "no activity,
   * try a longer range" there is wrong twice over: the Recent-transactions
   * table right below is full of trades from the same period, and no range
   * can fix a missing price.
   *
   * `tvl` is the discriminator because it is unambiguous — reserves exist
   * on every snapshot, so a null `tvl` can only mean a leg has no price. A
   * null `volume` is ambiguous by design (it also means "no swaps in this
   * bucket"), which is why the check does not use the selected metric.
   *
   * The copy says "in this period" rather than "this asset has no price
   * feed": the same pool can price fine on one range and not another — a
   * provider outage does exactly that (the 2026-07-21..08-03 freeze left a
   * 12-day hole in otherwise healthy series), and so does an asset whose
   * feed simply starts later than the window.
   */
  const unpriceable = useMemo(() => {
    if (!data || data.data_points.length === 0) return false;
    if (points.length > 0) return false;
    return data.data_points.every((row) => row.tvl == null);
  }, [data, points]);

  // Latest point drives the chart heading (Figma `325:22339` headline
  // shows the last value + its bucket date).
  const latest =
    points.length > 0 ? points[points.length - 1] ?? undefined : undefined;
  const headline = latest ? formatUsd(latest.value, USD_FULL_FORMATTER) : '—';
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
          alignItems: 'flex-end',
          flexWrap: 'wrap',
          gap: 1,
          px: 2,
          pt: 1,
          backgroundColor: theme.palette.surface.grayMainAlt,
          borderBottom: `1px solid ${theme.palette.stroke.default}`,
        })}
      >
        <Tabs
          tabs={[...TABS]}
          activeKey={metric}
          onChange={(key) => setMetric(key as ChartMetric)}
          aria-label="Chart metric"
        />

        <Box sx={{ mb: 1 }}>
          <IntervalPills
            intervals={PERIODS}
            active={period}
            onChange={setPeriod}
          />
        </Box>
      </Box>

      <Box sx={{ p: 2 }}>
        {isError ? (
          <QueryErrorState
            error={error}
            onRetry={() => void refetch()}
            py={4}
          />
        ) : (
          <TimeSeriesChart
            data={points}
            // Per-metric rendering — see METRIC_RENDERING. This departs
            // from Figma node `325:24354` (flat line, a hollow mark on
            // every bucket): static marks are gone entirely (task 0505 —
            // only the hover dot remains) and the fill is a fade rather
            // than the solid wash that made `area` unusable before.
            variant={METRIC_RENDERING[metric].variant}
            curve={METRIC_RENDERING[metric].curve}
            valueFormatter={usdFormatter}
            title={headline}
            subtitle={headlineCaption}
            // Range pills live in the card header above — suppress the
            // chart's built-in interval row so we don't get two copies.
            intervals={[]}
            loading={isLoading}
            emptyState={
              unpriceable ? (
                <ChartEmptyState
                  title="USD values unavailable"
                  hint="We have no price data for this pool's assets in this period, so its activity can't be shown in USD."
                />
              ) : (
                <ChartEmptyState
                  title="No activity in this period"
                  hint="Try a longer range."
                />
              )
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
 * The pre-0199 `CHARTS_ENABLED` kill-switch and the "pending the price
 * oracle" placeholder are gone: the chart endpoint returns real USD
 * series (task 0199 compute-at-read). An unpriceable pool gets its own
 * empty state saying so — see `unpriceable`.
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
