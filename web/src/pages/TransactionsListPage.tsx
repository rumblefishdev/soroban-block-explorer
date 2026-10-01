import { Stack } from '@mui/material';
import type { ListTransactionsData } from '@rumblefish/api-types';
import {
  isAccountId,
  isContractId,
  useCursorPagination,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo } from 'react';

import { PAGE_SIZE, useTransactionsList, usePagedRows } from '../api/index.js';

import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';
import { normalizeOperationType } from './transactions/operationTypes.js';
import { TransactionFilters } from './transactions/TransactionFilters.js';
import { TransactionsTable } from './transactions/TransactionsTable.js';

type Filters = NonNullable<ListTransactionsData['query']>;

export default function TransactionsListPage() {
  const { state, cursor, goNext, goPrev, setFilter, clearFilters } =
    useCursorPagination({
      filterKeys: ['q', 'op'],
    });
  const q = state.filters.q ?? '';
  // Normalise the URL `op` param against the backend enum — see
  // `normalizeOperationType` for the why. Bad / lowercase values
  // collapse to '' so the API never sees them.
  const op = normalizeOperationType(state.filters.op);
  const hasFilters = q !== '' || op !== '';

  // Map the combined search box to the API's separate account / contract
  // filters by inspecting the StrKey prefix. Unrecognised input applies no
  // filter rather than sending a value the API would reject.
  const queryFilters = useMemo<Filters>(() => {
    const filters: Filters = { limit: PAGE_SIZE };
    if (q) {
      if (isAccountId(q)) filters['filter[source_account]'] = q;
      else if (isContractId(q)) filters['filter[contract_id]'] = q;
    }
    if (op) filters['filter[operation_type]'] = op;
    return filters;
  }, [q, op]);

  const query = useTransactionsList(cursor, queryFilters);
  const pager = usePagedRows(query.data, goNext, goPrev);

  const handleSearchChange = useCallback(
    (value: string) => setFilter('q', value || null),
    [setFilter]
  );
  const handleOperationTypeChange = useCallback(
    (value: string) => setFilter('op', value || null),
    [setFilter]
  );

  return (
    <Stack spacing={3}>
      <PageHeader
        title="Transactions list"
        subtitle="All indexed transactions on the Stellar network"
      />
      <DataListCard
        filters={
          <TransactionFilters
            search={q}
            operationType={op}
            onSearchChange={handleSearchChange}
            onOperationTypeChange={handleOperationTypeChange}
          />
        }
        query={query}
        pager={pager}
        renderTable={(rows, { loading }) => (
          <TransactionsTable
            rows={rows}
            loading={loading}
            skeletonRows={PAGE_SIZE}
          />
        )}
        hasActiveFilters={hasFilters}
        emptyKind="transactions"
        emptyNoun="transactions"
        onClearFilters={clearFilters}
        paginationCaption="All results"
      />
    </Stack>
  );
}
