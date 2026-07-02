import { Box, Card, Skeleton, Typography } from '@mui/material';
import {
  LiveNowProvider,
  QueryErrorState,
  TableEmptyState,
  TableSectionHeader,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { useLatestLedgers } from '../../api/index.js';
import { routes } from '../../router/routes.js';
import { LedgersTable } from '../ledgers/LedgersTable.js';

import { LiveIndicator } from './LiveIndicator.js';
import { ViewAllLink } from './ViewAllLink.js';

/**
 * Latest Ledgers section — the 10 newest ledgers and a "View All" link to
 * the full Ledgers list. Reuses the shared `LedgersTable` (same columns as
 * the Figma home design: sequence, hash, closed-at, protocol, tx count).
 */
export function LatestLedgers() {
  const { data, dataUpdatedAt, isLoading, isError, error, refetch } =
    useLatestLedgers();
  const rows = data?.data ?? [];

  // Rows first: a transient failed poll must not blank a populated table —
  // keep showing the last good rows; the full-size error state is reserved
  // for "no data at all".
  let body: ReactNode;
  if (isLoading) {
    body = <LedgersTable rows={[]} loading skeletonRows={10} />;
  } else if (isError) {
    body = (
      <QueryErrorState error={error} onRetry={() => void refetch()} py={8} />
    );
  } else if (rows.length === 0) {
    body = <TableEmptyState kind="ledgers" />;
  } else {
    body = (
      <LiveNowProvider dataUpdatedAt={dataUpdatedAt}>
        <LedgersTable rows={rows} />
      </LiveNowProvider>
    );
  }

  return (
    <Card>
      <TableSectionHeader
        title="Latest Ledgers"
        badge={<LiveIndicator />}
        action={<ViewAllLink to={routes.ledgers} />}
      />
      <Box>{body}</Box>
      {/* Footer is rendered during loading too (with a skeleton count) so its
          height is reserved — otherwise the card jumps when the "N latest
          records" row appears with the data. */}
      {(rows.length > 0 || isLoading) && (
        <Box
          sx={{
            px: 2,
            py: 1.5,
            borderTop: (theme) => `1px solid ${theme.palette.stroke.default}`,
            backgroundColor: (theme) => theme.palette.surface.grayMainAlt,
          }}
        >
          {isLoading ? (
            <Skeleton variant="text" width={120} />
          ) : (
            <Typography
              component="span"
              variant="bodySmRegular"
              sx={(theme) => ({ color: theme.palette.text.tertiary })}
            >
              {rows.length} latest records
            </Typography>
          )}
        </Box>
      )}
    </Card>
  );
}
