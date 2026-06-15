import { Box, Card, Typography } from '@mui/material';
import {
  QueryErrorState,
  TableEmptyState,
  TableSectionHeader,
  TableSkeleton,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { useLatestLedgers } from '../../api/index.js';
import { routes } from '../../router/routes.js';
import { LEDGER_COLUMN_COUNT, LedgersTable } from '../ledgers/LedgersTable.js';

import { LiveIndicator } from './LiveIndicator.js';
import { ViewAllLink } from './ViewAllLink.js';

/**
 * Latest Ledgers section — the 10 newest ledgers and a "View All" link to
 * the full Ledgers list. Reuses the shared `LedgersTable` (same columns as
 * the Figma home design: sequence, hash, closed-at, protocol, tx count).
 */
export function LatestLedgers() {
  const { data, isLoading, isError, error, refetch } = useLatestLedgers();
  const rows = data?.data ?? [];

  let body: ReactNode;
  if (isLoading) {
    body = <TableSkeleton rows={10} columns={LEDGER_COLUMN_COUNT} />;
  } else if (isError) {
    body = (
      <QueryErrorState error={error} onRetry={() => void refetch()} py={8} />
    );
  } else if (rows.length === 0) {
    body = <TableEmptyState kind="ledgers" />;
  } else {
    body = <LedgersTable rows={rows} />;
  }

  return (
    <Card>
      <TableSectionHeader
        title="Latest Ledgers"
        badge={<LiveIndicator />}
        action={<ViewAllLink to={routes.ledgers} />}
      />
      <Box>{body}</Box>
      {rows.length > 0 && (
        <Box
          sx={{
            px: 2,
            py: 1.5,
            borderTop: (theme) => `1px solid ${theme.palette.stroke.default}`,
            backgroundColor: (theme) => theme.palette.surface.grayMainAlt,
          }}
        >
          <Typography
            component="span"
            variant="bodySmRegular"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            {rows.length} latest records
          </Typography>
        </Box>
      )}
    </Card>
  );
}
