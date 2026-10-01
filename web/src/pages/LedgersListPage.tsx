import { Stack } from '@mui/material';
import {
  type SortDirection,
  useCursorPagination,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback } from 'react';

import { useLedgersList, usePagedRows } from '../api/index.js';

import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';
import { LedgersTable } from './ledgers/LedgersTable.js';

export default function LedgersListPage() {
  const { state, cursor, goNext, goPrev, setSort } = useCursorPagination();
  // Sort lives in the URL `sort` (column) + `dir` (direction) params via
  // `setSort`, so it survives reload / deep links and stays paired with
  // the cursor.
  const sortDir = state.sortDir;
  const query = useLedgersList(cursor, sortDir);

  const handleSortChange = useCallback(
    // Column id comes from the table; `setSort` writes `?sort=&dir=` and
    // resets the cursor.
    (id: string, next: SortDirection) => setSort(id, next),
    [setSort]
  );

  const pager = usePagedRows(query.data, goNext, goPrev);

  return (
    <Stack spacing={3}>
      <PageHeader
        title="Ledgers"
        subtitle="All indexed ledgers on the Stellar network"
      />
      <DataListCard
        query={query}
        pager={pager}
        renderTable={(rows, { loading }) => (
          <LedgersTable
            rows={rows}
            loading={loading}
            skeletonRows={20}
            // As before, the skeleton's header neither shows nor changes the sort.
            {...(loading ? {} : { sortDir, onSortChange: handleSortChange })}
          />
        )}
        emptyKind="ledgers"
      />
    </Stack>
  );
}
