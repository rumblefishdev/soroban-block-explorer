import { Box, Card, Typography } from '@mui/material';
import {
  LiveNowProvider,
  PollingIndicator,
  QueryErrorState,
  TableEmptyState,
  TableSectionHeader,
  TableSkeleton,
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
 * Latest Transactions section — the 10 newest transactions with a live
 * indicator and a "View All" link to the full Transactions list.
 */
export function LatestTransactions() {
  const { data, dataUpdatedAt, isLoading, isError, error, refetch } =
    useLatestTransactions();
  const rows = data?.data ?? [];
  // "Updated …" is anchored to react-query's `dataUpdatedAt` — the moment of
  // the last successful poll — so it resets toward "just now" on every
  // refetch (its literal meaning). A genuinely stalled feed still surfaces:
  // a failed poll stops bumping `dataUpdatedAt`, and a feed returning the
  // same rows shows its age in every row's relative time.

  let body: ReactNode;
  if (isLoading) {
    body = <TableSkeleton rows={10} columns={LATEST_TX_COLUMN_COUNT} />;
  } else if (isError) {
    body = (
      <QueryErrorState error={error} onRetry={() => void refetch()} py={8} />
    );
  } else if (rows.length === 0) {
    body = <TableEmptyState kind="transactions" />;
  } else {
    body = (
      <LiveNowProvider dataUpdatedAt={dataUpdatedAt}>
        <LatestTransactionsTable rows={rows} />
      </LiveNowProvider>
    );
  }

  return (
    <Card>
      <TableSectionHeader
        title="Latest transactions"
        badge={<LiveIndicator />}
        description={<PollingIndicator lastUpdated={dataUpdatedAt} />}
        action={<ViewAllLink to={routes.transactions} />}
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
