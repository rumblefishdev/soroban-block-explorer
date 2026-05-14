import { Tooltip, Typography, type TypographyProps } from '@mui/material';

import { formatRelative } from './formatRelative.js';
import { useNow } from './useNow.js';

interface RelativeTimestampProps {
  timestamp: Date | string | number;
  intervalMs?: number;
  variant?: TypographyProps['variant'];
}

export function RelativeTimestamp({
  timestamp,
  intervalMs = 30_000,
  variant = 'bodySmRegular',
}: RelativeTimestampProps) {
  const now = useNow(intervalMs);
  const iso =
    timestamp instanceof Date
      ? timestamp.toISOString()
      : typeof timestamp === 'number'
      ? new Date(timestamp).toISOString()
      : timestamp;
  return (
    <Tooltip title={iso} arrow>
      <Typography
        component="span"
        variant={variant}
        sx={{ color: 'text.secondary', cursor: 'help' }}
      >
        {formatRelative(timestamp, now)}
      </Typography>
    </Tooltip>
  );
}
