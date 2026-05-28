import { Box, Typography } from '@mui/material';
import { useNow } from '@rumblefish/soroban-block-explorer-ui';

import { useNetworkStats } from '../../api/index.js';

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

/** ~4 ledger periods of slack before we call the feed delayed. */
const LIVE_MAX_AGE_MS = 20_000;

type LiveStatus = 'live' | 'delayed' | 'offline';

const STATUS_LABEL: Record<LiveStatus, string> = {
  live: 'LIVE',
  delayed: 'DELAYED',
  offline: 'OFFLINE',
};

const STATUS_DOT_COLOR: Record<LiveStatus, string> = {
  live: 'stroke.success',
  delayed: 'stroke.warning',
  offline: 'stroke.error',
};

export function LiveIndicator() {
  const { data, isError } = useNetworkStats();
  const now = useNow(1_000);

  let status: LiveStatus = 'live';
  if (isError) {
    status = 'offline';
  } else {
    const closedAt = data?.latest_ledger_closed_at;
    const closedMs = closedAt ? new Date(closedAt).getTime() : NaN;
    if (
      Number.isFinite(closedMs) &&
      now.getTime() - closedMs > LIVE_MAX_AGE_MS
    ) {
      status = 'delayed';
    }
  }

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
