import { Box, Skeleton, Stack, Typography } from '@mui/material';
import { BarChart } from '@mui/x-charts/BarChart';
import { LineChart } from '@mui/x-charts/LineChart';
import type { ReactNode } from 'react';
import { useId, useMemo } from 'react';

import { scales } from '../theme/colors.js';

export interface TimeSeriesPoint {
  timestamp: number | string | Date;
  value: number;
}

/**
 * Interpolation between points — a deliberately narrow subset of the
 * `@mui/x-charts` curve list, one per shape of data we actually plot:
 *
 * - `stepAfter` — a STATE that holds until the next event (TVL: it only
 *   moves on a deposit / withdraw / swap and is flat in between).
 * - `linear` — a measurement that genuinely varies continuously.
 * - `monotoneX` — smoothed. Pretty, but it invents intermediate values,
 *   so it belongs only on data where the in-between really is a curve.
 */
export type TimeSeriesCurve = 'linear' | 'monotoneX' | 'stepAfter';

/**
 * Y-axis gutter. Sized for the longest label a money formatter realistically
 * produces — sub-cent amounts need significant digits ("$0.000090"), and at
 * the previous 64px those ticks were ellipsised to "$0.000…", i.e. an axis
 * that showed nothing.
 */
const Y_AXIS_WIDTH = 78;

export interface TimeSeriesInterval {
  /** Stable key passed back to `onIntervalChange`. */
  key: string;
  label: ReactNode;
}

/**
 * Default range presets per Figma (LP detail chart panel). These are time
 * ranges, not bucket granularity — mapping a preset to the API `interval` /
 * `from` / `to` params is the consuming page's responsibility.
 */
export const DEFAULT_TIME_SERIES_INTERVALS: readonly TimeSeriesInterval[] = [
  { key: '1D', label: '1D' },
  { key: '7D', label: '7D' },
  { key: '30D', label: '30D' },
  { key: '1Y', label: '1Y' },
];

export interface TimeSeriesChartProps {
  data: readonly TimeSeriesPoint[];
  /**
   * How to draw the series. Defaults to `'line'`.
   *
   * Pick by what the numbers ARE, not by looks: `line`/`area` for a state
   * sampled over time, **`bar` for a flow** — a per-bucket sum such as
   * volume or fees. A line between two independent sums implies the value
   * passed through the points in between, which is meaningless, and it
   * also bridges straight over gaps the caller dropped, hiding them.
   * Bars just leave the gap empty.
   */
  variant?: 'line' | 'area' | 'bar';
  /** Interpolation for `line` / `area`; ignored by `bar`. See
   *  [`TimeSeriesCurve`]. Defaults to the chart library's smoothing. */
  curve?: TimeSeriesCurve;
  /** Headline value, e.g. "$2,400,000". */
  title?: ReactNode;
  /** Secondary line under the title, e.g. "Apr 13, 2026 · TVL". */
  subtitle?: ReactNode;
  /** Chart body height in px. Defaults to 300. */
  height?: number;
  /** Formats values for the y-axis and hover tooltip. */
  valueFormatter?: (value: number) => string;
  /** Range-preset options. Defaults to DEFAULT_TIME_SERIES_INTERVALS. */
  intervals?: readonly TimeSeriesInterval[];
  /** Active preset key. */
  activeInterval?: string;
  /** Fired when a preset is picked — the page re-fetches with the new range. */
  onIntervalChange?: (key: string) => void;
  loading?: boolean;
  /** Rendered when `data` is empty and not loading. */
  emptyState?: ReactNode;
}

function IntervalSelector({
  intervals,
  activeInterval,
  onIntervalChange,
}: {
  intervals: readonly TimeSeriesInterval[];
  activeInterval?: string;
  onIntervalChange?: (key: string) => void;
}) {
  return (
    <Stack direction="row" spacing={0.5} role="group">
      {intervals.map((interval) => {
        const active = interval.key === activeInterval;
        return (
          <Box
            key={interval.key}
            component="button"
            type="button"
            aria-pressed={active}
            onClick={() => onIntervalChange?.(interval.key)}
            sx={(theme) => ({
              ...theme.typography.bodyXsMedium,
              border: 'none',
              borderRadius: `${theme.shape.radius.s}px`,
              px: 1.25,
              py: 0.5,
              cursor: 'pointer',
              backgroundColor: active
                ? theme.palette.surface.primaryMain
                : theme.palette.surface.grayLight,
              color: active
                ? theme.palette.grey[900]
                : theme.palette.text.secondary,
              '&:hover': {
                backgroundColor: active
                  ? theme.palette.surface.primaryHover
                  : theme.palette.surface.grayHover,
              },
              '&:focus-visible': {
                outline: `2px solid ${theme.palette.stroke.action}`,
                outlineOffset: 2,
              },
            })}
          >
            {interval.label}
          </Box>
        );
      })}
    </Stack>
  );
}

/**
 * Presentational time-series line/area chart built on `@mui/x-charts`.
 * Controlled: the interval selector emits `onIntervalChange`; the parent owns
 * data fetching. Wrap in `LazySection` to defer rendering until visible.
 */
export function TimeSeriesChart({
  data,
  variant = 'line',
  curve,
  title,
  subtitle,
  height = 300,
  valueFormatter,
  intervals = DEFAULT_TIME_SERIES_INTERVALS,
  activeInterval,
  onIntervalChange,
  loading = false,
  emptyState,
}: TimeSeriesChartProps) {
  const formatValue = (value: number | null): string => {
    if (value == null) return '';
    return valueFormatter ? valueFormatter(value) : String(value);
  };

  const xData = useMemo(
    () => data.map((point) => new Date(point.timestamp)),
    [data]
  );
  const yData = useMemo(() => data.map((point) => point.value), [data]);
  const isEmpty = data.length === 0;

  const isBar = variant === 'bar';
  // Anything with a FILL is read by area, so its baseline must be zero:
  // from a cropped baseline the fill of a $130 value next to a $570 one
  // looks like a sliver next to a wall, when the real ratio is 4×. A bare
  // line makes no such claim, so it keeps the zoomed range that keeps
  // small movements legible — that is the trade the caller picks by
  // choosing `line` over `area`.
  const isFilled = variant === 'bar' || variant === 'area';
  // Unique per instance — two charts on one page must not share a <defs> id.
  const areaGradientId = `${useId()}-area-fill`;
  const { yMin, yMax } = useMemo(() => {
    if (yData.length === 0) return { yMin: undefined, yMax: undefined };
    const min = Math.min(...yData);
    const max = Math.max(...yData);
    const pad = Math.max((max - min) * 0.1, max * 0.001);
    return { yMin: isFilled ? 0 : min - pad, yMax: max + pad };
  }, [yData, isFilled]);

  const xSpanMs =
    xData.length > 1
      ? xData[xData.length - 1]!.getTime() - xData[0]!.getTime()
      : 0;
  const formatXTick = (date: Date): string => {
    if (xSpanMs <= 36 * 60 * 60 * 1000) {
      return date.toLocaleTimeString('en-US', {
        hour: 'numeric',
        minute: '2-digit',
      });
    }
    if (xSpanMs <= 90 * 24 * 60 * 60 * 1000) {
      return date.toLocaleDateString('en-US', {
        month: 'short',
        day: 'numeric',
      });
    }
    return date.toLocaleDateString('en-US', {
      month: 'short',
      year: '2-digit',
    });
  };
  const formatXTooltip = (date: Date): string =>
    date.toLocaleString('en-US', {
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
    });

  return (
    <Stack spacing={2}>
      <Stack
        direction="row"
        alignItems="flex-start"
        justifyContent="space-between"
        spacing={2}
        flexWrap="wrap"
      >
        <Stack spacing={0.75}>
          {title !== undefined && (
            <Typography variant="heading6SemiBold" color="text.primary">
              {title}
            </Typography>
          )}
          {subtitle !== undefined && (
            <Typography variant="bodyMedium" color="text.secondary">
              {subtitle}
            </Typography>
          )}
        </Stack>
        {onIntervalChange && intervals.length > 0 && (
          <IntervalSelector
            intervals={intervals}
            activeInterval={activeInterval}
            onIntervalChange={onIntervalChange}
          />
        )}
      </Stack>

      {loading ? (
        <Skeleton variant="rounded" height={height} />
      ) : isEmpty ? (
        <Box
          sx={{
            display: 'flex',
            justifyContent: 'center',
            alignItems: 'center',
            height,
          }}
        >
          {emptyState ?? (
            <Typography variant="bodyRegular" color="text.secondary">
              No data for this range
            </Typography>
          )}
        </Box>
      ) : isBar ? (
        // A bar axis is BAND, not time: each bucket owns a slot of equal
        // width. That is the point — a bucket with no bar is a visible hole
        // rather than a segment drawn straight over it.
        <BarChart
          height={height}
          hideLegend
          grid={{ horizontal: true }}
          margin={{ left: 56, right: 16, top: 8, bottom: 8 }}
          borderRadius={4}
          xAxis={[
            {
              data: xData,
              scaleType: 'band',
              valueFormatter: (date: Date, ctx) =>
                ctx.location === 'tick'
                  ? formatXTick(date)
                  : formatXTooltip(date),
            },
          ]}
          yAxis={[
            {
              valueFormatter: (value: number) => formatValue(value),
              min: yMin,
              max: yMax,
              domainLimit: 'nice',
              width: Y_AXIS_WIDTH,
            },
          ]}
          series={[
            {
              data: yData,
              color: scales.blue[400],
              valueFormatter: formatValue,
            },
          ]}
        />
      ) : (
        <LineChart
          height={height}
          hideLegend
          grid={{ horizontal: true }}
          margin={{ left: 56, right: 16, top: 8, bottom: 8 }}
          xAxis={[
            {
              data: xData,
              scaleType: 'time',

              tickNumber: 6,
              valueFormatter: (date: Date, ctx) =>
                ctx.location === 'tick'
                  ? formatXTick(date)
                  : formatXTooltip(date),
            },
          ]}
          yAxis={[
            {
              valueFormatter: (value: number) => formatValue(value),

              min: yMin,
              max: yMax,
              domainLimit: 'nice',

              width: Y_AXIS_WIDTH,
            },
          ]}
          series={[
            {
              data: yData,
              area: variant === 'area',
              showMark: true,
              ...(curve ? { curve } : {}),
              color: scales.blue[400],
              valueFormatter: formatValue,
            },
          ]}
          sx={{
            '&& .MuiLineChart-line': {
              strokeWidth: 2,
            },
            // Fade the fill out toward the baseline: a flat wash reads as a
            // solid block and buries the gridlines, while a gradient keeps
            // the line the subject and the fill a magnitude cue.
            '&& .MuiAreaElement-root': {
              fill: `url(#${areaGradientId})`,
            },
            // Marks stay on at EVERY density — a threshold that hid them
            // past N points made the same range render differently on two
            // pools (measured: a 7-day window is 25 points on one pool and
            // 41 on another), which reads as two different chart types.
            //
            // What actually caused the beading was their size, not the
            // count: r:3 with a 6px halo is ~12px across, and 52 weekly
            // points on a ~570px plot sit ~11px apart, so they could not
            // help but touch. At ~4px they stay separate at that density
            // and still mark where a real sample is on a flat series.
            '&& .MuiLineChart-mark': {
              fill: scales.blue[100],
              stroke: `${scales.blue[400]}aa`,
              strokeWidth: 2,
              r: 2,
            },
          }}
        >
          {variant === 'area' && (
            <defs>
              <linearGradient id={areaGradientId} x1="0" y1="0" x2="0" y2="1">
                <stop
                  offset="0%"
                  stopColor={scales.blue[400]}
                  stopOpacity={0.32}
                />
                <stop
                  offset="100%"
                  stopColor={scales.blue[400]}
                  stopOpacity={0}
                />
              </linearGradient>
            </defs>
          )}
        </LineChart>
      )}
    </Stack>
  );
}
