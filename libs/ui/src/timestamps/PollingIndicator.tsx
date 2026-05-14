import RefreshIcon from '@mui/icons-material/Refresh';
import { Stack, Typography } from '@mui/material';

import { formatRelative } from './formatRelative.js';
import { useNow } from './useNow.js';

interface PollingIndicatorProps {
  lastUpdated: Date | string | number;
  intervalMs?: number;
}

export function PollingIndicator({
  lastUpdated,
  intervalMs = 5_000,
}: PollingIndicatorProps) {
  const now = useNow(intervalMs);
  return (
    <Stack
      direction="row"
      spacing={0.5}
      alignItems="center"
      sx={{ color: 'text.tertiary' }}
    >
      <RefreshIcon sx={{ fontSize: 14 }} />
      <Typography variant="bodyXsRegular" component="span">
        Updated {formatRelative(lastUpdated, now)}
      </Typography>
    </Stack>
  );
}
