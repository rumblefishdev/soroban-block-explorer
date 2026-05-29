import { Box, Skeleton, Stack, Typography } from '@mui/material';
import { LineChart } from '@mui/x-charts/LineChart';
import type { ReactNode } from 'react';
import { useMemo } from 'react';

import { scales } from '../theme/colors.js';

export interface TimeSeriesPoint {
  timestamp: number | string | Date;
  value: number;
}

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
  /** Line or filled-area rendering. Defaults to 'line'. */
  variant?: 'line' | 'area';
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

  const { yMin, yMax } = useMemo(() => {
    if (yData.length === 0) return { yMin: undefined, yMax: undefined };
    const min = Math.min(...yData);
    const max = Math.max(...yData);
    const pad = Math.max((max - min) * 0.1, max * 0.001);
    return { yMin: min - pad, yMax: max + pad };
  }, [yData]);

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
              valueFormatter: (date: Date, ctx) => {
                if (ctx.location === 'tick') return formatXTick(date);
                return date.toLocaleString('en-US', {
                  month: 'short',
                  day: 'numeric',
                  hour: 'numeric',
                  minute: '2-digit',
                });
              },
            },
          ]}
          yAxis={[
            {
              valueFormatter: (value: number) => formatValue(value),

              min: yMin,
              max: yMax,
              domainLimit: 'nice',

              width: 64,
            },
          ]}
          series={[
            {
              data: yData,
              area: variant === 'area',
              showMark: true,
              color: scales.blue[400],
              valueFormatter: formatValue,
            },
          ]}
          sx={{
            '&& .MuiLineChart-line': {
              strokeWidth: 2,
            },
            '&& .MuiLineChart-mark': {
              fill: scales.blue[100],

              stroke: `${scales.blue[400]}aa`,
              strokeWidth: 6,
              r: 3,
            },
          }}
        />
      )}
    </Stack>
  );
}
