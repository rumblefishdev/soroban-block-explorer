import { Skeleton, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

interface ChainOverviewCardProps {
  /** Top-of-card label — plain text, or a node such as the LIVE indicator. */
  label: ReactNode;
  /** Big primary value. Omitted while `loading` is true. */
  value?: string;
  /** Caption shown under the value. */
  caption: string;
  /** Accent the value colour (used for TPS). */
  accentValue?: boolean;
  loading?: boolean;
}

/** One cell of the chain-overview panel: label, large value, caption. */
export function ChainOverviewCard({
  label,
  value,
  caption,
  accentValue = false,
  loading = false,
}: ChainOverviewCardProps) {
  return (
    <Stack spacing={1} alignItems="center" sx={{ flex: 1, px: 2, py: 3 }}>
      <Typography
        component="span"
        variant="bodySmMedium"
        sx={{ color: 'text.secondary' }}
      >
        {label}
      </Typography>
      {loading ? (
        <Skeleton variant="text" width={140} sx={{ fontSize: 32 }} />
      ) : (
        <Typography
          component="span"
          variant="heading3Bold"
          sx={{ color: accentValue ? 'text.success' : 'text.primary' }}
        >
          {value ?? '—'}
        </Typography>
      )}
      <Typography
        component="span"
        variant="bodyXsRegular"
        sx={{ color: 'text.tertiary' }}
      >
        {caption}
      </Typography>
    </Stack>
  );
}
