import { Box, Typography } from '@mui/material';

import { useLiveStatus, type LiveStatus } from '../../api/index.js';

/**
 * Live status pip shown on the chain-overview panel and the activity-
 * table section headers. Driven by `NetworkStats.latest_ledger_closed_at`:
 *
 * - **LIVE** — newest ledger closed within `LIVE_MAX_AGE_MS` and no error
 * - **DELAYED** — data present but the chain (or our polling) is behind
 * - **OFFLINE** — the stats query is erroring
 *
 * It stays visible in every state on purpose — a disappearing pip reads
 * as "broken / frozen" rather than "data is stale".
 */

const STATUS_LABEL: Record<LiveStatus, string> = {
  live: 'LIVE',
  delayed: 'STALE',
  offline: 'OFFLINE',
};

const STATUS_DOT_COLOR: Record<LiveStatus, string> = {
  live: 'stroke.success',
  delayed: 'stroke.warning',
  offline: 'stroke.error',
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
        sx={{
          width: 6,
          height: 6,
          borderRadius: '50%',
          backgroundColor: STATUS_DOT_COLOR[status],
        }}
      />
      <Typography
        component="span"
        variant="bodyXsMedium"
        sx={{ color: 'text.secondary', letterSpacing: '0.06em' }}
      >
        {STATUS_LABEL[status]}
      </Typography>
    </Box>
  );
}
