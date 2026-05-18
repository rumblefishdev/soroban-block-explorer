import { Box, Skeleton, Stack, Typography } from '@mui/material';
import { LineChart } from '@mui/x-charts/LineChart';
import type { ReactNode } from 'react';

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

  const xData = data.map((point) => new Date(point.timestamp));
  const yData = data.map((point) => point.value);
  const isEmpty = data.length === 0;

  return (
    <Stack spacing={2}>
      <Stack
        direction="row"
        alignItems="flex-start"
        justifyContent="space-between"
        spacing={2}
        flexWrap="wrap"
      >
        <Stack spacing={0.25}>
          {title !== undefined && (
            <Typography variant="heading5SemiBold" color="text.primary">
              {title}
            </Typography>
          )}
          {subtitle !== undefined && (
            <Typography variant="bodySmRegular" color="text.secondary">
              {subtitle}
            </Typography>
          )}
        </Stack>
        {intervals.length > 0 && (
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
          margin={{ left: 12, right: 12, top: 8, bottom: 8 }}
          xAxis={[{ data: xData, scaleType: 'time' }]}
          yAxis={[{ valueFormatter: (value: number) => formatValue(value) }]}
          series={[
            {
              data: yData,
              area: variant === 'area',
              showMark: true,
              color: scales.blue[600],
              valueFormatter: formatValue,
            },
          ]}
          sx={{
            // Figma: the data line carries a soft blue glow.
            '& .MuiLineElement-root': {
              filter: `drop-shadow(0px 3px 8px ${scales.blue[600]}59)`,
            },
          }}
        />
      )}
    </Stack>
  );
}
