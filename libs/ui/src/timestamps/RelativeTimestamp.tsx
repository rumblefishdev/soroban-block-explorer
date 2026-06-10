import { Tooltip, Typography, type TypographyProps } from '@mui/material';

import { formatRelative } from './formatRelative.js';
import { useNow } from './useNow.js';

interface RelativeTimestampProps {
  timestamp: Date | string | number;
  variant?: TypographyProps['variant'];
}

const FALLBACK = '—';

function toIso(value: Date | string | number): string | null {
  const d = value instanceof Date ? value : new Date(value);
  const ms = d.getTime();
  return Number.isFinite(ms) && ms > 0 ? d.toISOString() : null;
}

export function RelativeTimestamp({
  timestamp,
  variant = 'bodySmRegular',
}: RelativeTimestampProps) {
  const now = useNow();
  const iso = toIso(timestamp);
  const label = iso ? formatRelative(timestamp, now) : FALLBACK;

  return (
    <Tooltip title={iso ?? FALLBACK} arrow disableHoverListener={!iso}>
      <Typography
        component="span"
        variant={variant}
        sx={{ color: 'text.secondary' }}
      >
        {label}
      </Typography>
    </Tooltip>
  );
}
