import { Box, Card, Stack, Typography } from '@mui/material';
import {
  classifyError,
  GenericErrorState,
  PaginationControls,
  RateLimitState,
  TableEmptyState,
  TableSkeleton,
  TransientErrorState,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useState, type ReactNode } from 'react';

import { useLedgersList } from '../api/index.js';

import { LEDGER_COLUMN_COUNT, LedgersTable } from './ledgers/LedgersTable.js';

export default function LedgersListPage() {
  const {
    data,
    isLoading,
    isError,
    error,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    refetch,
  } = useLedgersList();

  const [pageIndex, setPageIndex] = useState(0);

  const pages = data?.pages ?? [];
  const currentPage = pages[pageIndex];
  const rows = currentPage?.data ?? [];
  const canPrev = pageIndex > 0;
  const canNext = Boolean(currentPage?.page.has_more);

  const handlePrev = useCallback(() => {
    setPageIndex((index) => Math.max(0, index - 1));
  }, []);

  const handleNext = useCallback(() => {
    if (isFetchingNextPage) return;
    if (pageIndex + 1 < pages.length) {
      setPageIndex(pageIndex + 1);
      return;
    }
    if (hasNextPage) {
      void fetchNextPage().then(() => setPageIndex((index) => index + 1));
    }
  }, [pageIndex, pages.length, hasNextPage, isFetchingNextPage, fetchNextPage]);

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
          prevCursor={canPrev ? 'prev' : null}
          nextCursor={canNext ? 'next' : null}
          onPrev={handlePrev}
          onNext={handleNext}
        />
      </Card>
    </Stack>
  );
}
