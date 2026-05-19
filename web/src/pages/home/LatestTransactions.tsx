import { Box, Card, Typography } from '@mui/material';
import {
  classifyError,
  GenericErrorState,
  PollingIndicator,
  RateLimitState,
  TableEmptyState,
  TableSectionHeader,
  TableSkeleton,
  TransientErrorState,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { useLatestTransactions } from '../../api/index.js';
import { routes } from '../../router/routes.js';

import {
  LATEST_TX_COLUMN_COUNT,
  LatestTransactionsTable,
} from './LatestTransactionsTable.js';
import { LiveIndicator } from './LiveIndicator.js';
import { ViewAllLink } from './ViewAllLink.js';

/**
 * Latest Transactions section — the 10 newest transactions with a polling
 * indicator and a "View All" link to the full Transactions list.
 */
export function LatestTransactions() {
  const { data, isLoading, isError, error, dataUpdatedAt, refetch } =
    useLatestTransactions();
  const rows = data?.data ?? [];

  let body: ReactNode;
  if (isLoading) {
    body = (
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={10} columns={LATEST_TX_COLUMN_COUNT} />
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
        <TableEmptyState kind="transactions" />
      </Box>
    );
  } else {
    body = <LatestTransactionsTable rows={rows} />;
  }

  return (
    <Box sx={{ px: 10 }}>
      <Card>
        <TableSectionHeader
          title="Latest transactions"
          badge={<LiveIndicator />}
          description={<PollingIndicator lastUpdated={dataUpdatedAt} />}
          action={<ViewAllLink to={routes.transactions} />}
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
            {rows.length} latest records
          </Typography>
        </Box>
      </Card>
    </Box>
  );
}
