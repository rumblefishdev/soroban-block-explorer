import { Stack, Typography } from '@mui/material';
import { formatRelative, useNow } from '@rumblefish/soroban-block-explorer-ui';

import { formatAbsoluteUtc } from './formatters.js';

interface TransactionTimeProps {
  createdAt: string;
}

/**
 * Two-line Time cell for the Transactions table: relative time on top
 * (`2 min ago`), absolute UTC underneath (`2026-04-13 14:23:51 UTC`).
 * Matches the Design System table Time column.
 */
export function TransactionTime({ createdAt }: TransactionTimeProps) {
  const now = useNow(30_000);
  const valid = Number.isFinite(new Date(createdAt).getTime());

  return (
    <Stack spacing={0.25}>
      <Typography
        component="span"
        variant="bodySmRegular"
        sx={(theme) => ({ color: theme.palette.text.primary })}
      >
        {valid ? formatRelative(createdAt, now) : '—'}
      </Typography>
      <Typography
        component="span"
        variant="bodyMonoXsRegular"
        sx={(theme) => ({ color: theme.palette.text.tertiary })}
      >
        {formatAbsoluteUtc(createdAt)}
      </Typography>
    </Stack>
  );
}
