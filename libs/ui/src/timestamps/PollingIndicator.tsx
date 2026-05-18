import RefreshIcon from '@mui/icons-material/Refresh';
import { Box, Typography } from '@mui/material';

import { formatRelative } from './formatRelative.js';
import { useNow } from './useNow.js';

interface PollingIndicatorProps {
  lastUpdated?: Date | string | number;
  intervalMs?: number;
  /** When true the refresh icon spins, signalling a poll is in flight. */
  isFetching?: boolean;
  /** When provided the indicator becomes a button that triggers a refetch. */
  onRefresh?: () => void;
}

function isReady(value: Date | string | number | undefined): boolean {
  if (value == null) return false;
  const ms =
    value instanceof Date ? value.getTime() : new Date(value).getTime();
  return Number.isFinite(ms) && ms > 0;
}

/**
 * Compact "Updated Xs ago" indicator for polling-enabled pages. Pass
 * `isFetching` to spin the icon during a refetch; pass `onRefresh` to make
 * the whole indicator a button that triggers an immediate refetch.
 */
export function PollingIndicator({
  lastUpdated,
  intervalMs = 5_000,
  isFetching = false,
  onRefresh,
}: PollingIndicatorProps) {
  const now = useNow(intervalMs);
  const interactive = typeof onRefresh === 'function';
  const label =
    lastUpdated != null && isReady(lastUpdated)
      ? `Updated ${formatRelative(lastUpdated, now)}`
      : 'Not updated yet';
  return (
    <Box
      component={interactive ? 'button' : 'span'}
      type={interactive ? 'button' : undefined}
      onClick={onRefresh}
      aria-label={interactive ? 'Refresh now' : undefined}
      sx={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 0.5,
        color: 'text.tertiary',
        ...(interactive && {
          border: 0,
          background: 'none',
          padding: 0,
          font: 'inherit',
          cursor: 'pointer',
          '&:hover': { color: 'text.secondary' },
          '&:focus-visible': {
            outline: (theme) => `2px solid ${theme.palette.stroke.action}`,
            outlineOffset: 2,
            borderRadius: 1,
          },
        }),
      }}
    >
      <RefreshIcon
        sx={{
          fontSize: 14,
          '@keyframes pollingSpin': { to: { transform: 'rotate(360deg)' } },
          animation: isFetching ? 'pollingSpin 1s linear infinite' : 'none',
        }}
      />
      <Typography variant="bodyXsRegular" component="span">
        {label}
      </Typography>
    </Box>
  );
}
