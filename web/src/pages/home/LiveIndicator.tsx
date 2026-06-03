import { Box, Typography } from '@mui/material';

import { useLiveStatus, type LiveStatus } from '../../api/index.js';

/**
 * Live status pip shown on the chain-overview panel and the activity-
 * table section headers. Driven by `NetworkStats.latest_ledger_closed_at`:
 *
 * - **LIVE** — newest ledger closed within `LIVE_MAX_AGE_MS` and no error
 * - **STALE** — data present but the chain (or our polling) is behind
 * - **OFFLINE** — the stats query is erroring
 * - **CONNECTING** — still loading / no usable timestamp yet
 *
 * It stays visible in every state on purpose — a disappearing pip reads
 * as "broken / frozen" rather than "data is stale".
 */

const STATUS_LABEL: Record<LiveStatus, string> = {
  live: 'LIVE',
  delayed: 'STALE',
  offline: 'OFFLINE',
  unknown: 'CONNECTING',
};

/** Maps each status to a `theme.palette.stroke` key — resolved through the
 *  theme (type-checked) rather than a magic `'stroke.x'` sx string. */
const STATUS_DOT_KEY: Record<
  LiveStatus,
  'success' | 'warning' | 'error' | 'default'
> = {
  live: 'success',
  delayed: 'warning',
  offline: 'error',
  unknown: 'default',
};

export function LiveIndicator() {
  const status = useLiveStatus();

  return (
    <Box
      component="span"
      sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.5 }}
    >
      <Box
        component="span"
        sx={(theme) => ({
          width: 6,
          height: 6,
          borderRadius: '50%',
          backgroundColor: theme.palette.stroke[STATUS_DOT_KEY[status]],
        })}
      />
      <Typography
        component="span"
        variant="bodyXsMedium"
        sx={(theme) => ({
          color: theme.palette.text.secondary,
          letterSpacing: '0.06em',
        })}
      >
        {STATUS_LABEL[status]}
      </Typography>
    </Box>
  );
}
