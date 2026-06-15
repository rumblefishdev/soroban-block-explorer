import { Stack } from '@mui/material';
import type { ListTransactionsData } from '@rumblefish/api-types';
import {
  isAccountId,
  isContractId,
  useCursorPagination,
  usePageHandlers,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo } from 'react';

import { useTransactionsList } from '../api/index.js';

import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';
import { normalizeOperationType } from './transactions/operationTypes.js';
import { TransactionFilters } from './transactions/TransactionFilters.js';
import {
  TRANSACTION_COLUMN_COUNT,
  TransactionsTable,
} from './transactions/TransactionsTable.js';

type Filters = NonNullable<ListTransactionsData['query']>;

const PAGE_SIZE = 20;

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

  const { data, isLoading, isError, error, refetch } = useTransactionsList(
    cursor,
    queryFilters
  );

  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );

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
        columnCount={TRANSACTION_COLUMN_COUNT}
        isLoading={isLoading}
        isError={isError}
        error={error}
        onRetry={() => void refetch()}
        rows={rows}
        renderTable={(visibleRows) => <TransactionsTable rows={visibleRows} />}
        hasActiveFilters={hasFilters}
        emptyKind="transactions"
        emptyNoun="transactions"
        onClearFilters={clearFilters}
        paginationCaption="All results"
        canPrev={canPrev}
        canNext={canNext}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </Stack>
  );
}
