import RefreshIcon from '@mui/icons-material/Refresh';
import { Stack, Typography } from '@mui/material';

import { formatRelative } from './formatRelative.js';
import { useNow } from './useNow.js';

interface PollingIndicatorProps {
  lastUpdated?: Date | string | number;
}

function isReady(value: Date | string | number | undefined): boolean {
  if (value == null) return false;
  const ms =
    value instanceof Date ? value.getTime() : new Date(value).getTime();
  return Number.isFinite(ms) && ms > 0;
}

export function PollingIndicator({ lastUpdated }: PollingIndicatorProps) {
  const now = useNow();
  const ready = isReady(lastUpdated);
  return (
    <Stack
      direction="row"
      spacing={0.5}
      alignItems="center"
      sx={{ color: 'text.tertiary' }}
    >
      <RefreshIcon sx={{ fontSize: 14 }} />
      <Typography variant="bodyXsRegular" component="span">
        {ready
          ? `Updated ${formatRelative(lastUpdated!, now)}`
          : 'Not updated yet'}
      </Typography>
    </Stack>
  );
}
