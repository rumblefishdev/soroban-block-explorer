import { Stack } from '@mui/material';
import {
  type SortDirection,
  useCursorPagination,
  usePageHandlers,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback } from 'react';

import { useLedgersList } from '../api/index.js';

import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';
import { LEDGER_COLUMN_COUNT, LedgersTable } from './ledgers/LedgersTable.js';

export default function LedgersListPage() {
  const { state, cursor, goNext, goPrev, setSort } = useCursorPagination();
  // Sort lives in the URL `sort` (column) + `dir` (direction) params via
  // `setSort`, so it survives reload / deep links and stays paired with
  // the cursor.
  const sortDir = state.sortDir;
  const { data, isLoading, isPlaceholderData, isError, error, refetch } =
    useLedgersList(cursor, sortDir);

  const handleSortChange = useCallback(
    // Column id comes from the table; `setSort` writes `?sort=&dir=` and
    // resets the cursor.
    (id: string, next: SortDirection) => setSort(id, next),
    [setSort]
  );

  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );

  return (
    <Stack spacing={3}>
      <PageHeader
        title="Ledgers"
        subtitle="All indexed ledgers on the Stellar network"
      />
      <DataListCard
        columnCount={LEDGER_COLUMN_COUNT}
        isLoading={isLoading}
        isReloading={isPlaceholderData}
        isError={isError}
        error={error}
        onRetry={() => void refetch()}
        rows={rows}
        renderTable={(visibleRows) => (
          <LedgersTable
            rows={visibleRows}
            sortDir={sortDir}
            onSortChange={handleSortChange}
          />
        )}
        renderSkeleton={() => (
          <LedgersTable rows={[]} loading skeletonRows={20} />
        )}
        emptyKind="ledgers"
        emptyNoun="ledgers"
        canPrev={canPrev}
        canNext={canNext}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </Stack>
  );
}
