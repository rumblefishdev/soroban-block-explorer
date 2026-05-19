import { Box, Card, Typography } from '@mui/material';
import {
  classifyError,
  GenericErrorState,
  RateLimitState,
  TableEmptyState,
  TableSectionHeader,
  TableSkeleton,
  TransientErrorState,
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
    body = (
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={10} columns={LEDGER_COLUMN_COUNT} />
      </Box>
    );
  } else if (isError) {
    const kind = classifyError(error);
    const retry = () => void refetch();
    body = (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 8 }}>
        {kind === 'rate-limit' ? (
          <RateLimitState onRetry={retry} />
        ) : kind === 'transient' ? (
          <TransientErrorState onRetry={retry} />
        ) : (
          <GenericErrorState onRetry={retry} />
        )}
      </Box>
    );
  } else if (rows.length === 0) {
    body = (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 8 }}>
        <TableEmptyState kind="ledgers" />
      </Box>
    );
  } else {
    body = <LedgersTable rows={rows} />;
  }

  return (
    <Box sx={{ px: 10 }}>
      <Card>
        <TableSectionHeader
          title="Latest Ledgers"
          badge={<LiveIndicator />}
          action={<ViewAllLink to={routes.ledgers} />}
        />
        <Box sx={{ minHeight: 320 }}>{body}</Box>
        <Box
          sx={{
            px: 2,
            py: 1.5,
            borderTop: (theme) => `1px solid ${theme.palette.stroke.default}`,
          }}
        >
          <Typography
            component="span"
            variant="bodySmRegular"
            sx={{ color: 'text.tertiary' }}
          >
            Latest {rows.length} results
          </Typography>
        </Box>
      </Card>
    </Box>
  );
}
