import { Stack } from '@mui/material';
import {
  type SortDirection,
  useCursorPagination,
  usePageHandlers,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useState } from 'react';

import { useLedgersList } from '../api/index.js';

import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';
import { LEDGER_COLUMN_COUNT, LedgersTable } from './ledgers/LedgersTable.js';

export default function LedgersListPage() {
  const { cursor, goNext, goPrev, reset } = useCursorPagination();
  const [sortDir, setSortDir] = useState<SortDirection>('desc');
  const { data, isLoading, isError, error, refetch } = useLedgersList(
    cursor,
    sortDir
  );

  const handleSortChange = useCallback(
    (next: SortDirection) => {
      setSortDir(next);
      reset();
    },
    [reset]
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
