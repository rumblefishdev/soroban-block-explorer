import { Box, Card, Stack, Typography } from '@mui/material';
import {
  classifyError,
  GenericErrorState,
  PaginationControls,
  RateLimitState,
  TableEmptyState,
  TableSkeleton,
  TransientErrorState,
  useCursorPagination,
  usePageHandlers,
} from '@rumblefish/soroban-block-explorer-ui';
import { type ReactNode } from 'react';

import { useLedgersList } from '../api/index.js';

import { LEDGER_COLUMN_COUNT, LedgersTable } from './ledgers/LedgersTable.js';

export default function LedgersListPage() {
  const { cursor, goNext, goPrev } = useCursorPagination();
  const { data, isLoading, isError, error, refetch } = useLedgersList(cursor);

  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );

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
    <Stack spacing={3}>
      <Box>
        <Typography variant="heading3SemiBold" component="h1">
          Ledgers
        </Typography>
        <Typography variant="bodyRegular" sx={{ color: 'text.secondary' }}>
          All indexed ledgers on the Stellar network
        </Typography>
      </Box>

      <Card>
        <Box sx={{ minHeight: 320 }}>{body}</Box>
        <PaginationControls
          caption="Latest results"
          canPrev={canPrev}
          canNext={canNext}
          onPrev={handlePrev}
          onNext={handleNext}
        />
      </Card>
    </Stack>
  );
}
