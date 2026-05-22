import SearchIcon from '@mui/icons-material/SearchOutlined';
import { Box, Button, Card, Stack, Typography } from '@mui/material';
import type { ListTransactionsData } from '@rumblefish/api-types';
import {
  classifyError,
  EmptyState,
  GenericErrorState,
  isAccountId,
  isContractId,
  PaginationControls,
  RateLimitState,
  TableEmptyState,
  TableSkeleton,
  TransientErrorState,
  useCursorPagination,
  usePageHandlers,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo, type ReactNode } from 'react';

import { useTransactionsList } from '../api/index.js';

import { normalizeOperationType } from './transactions/operationTypes.js';
import { TransactionFilters } from './transactions/TransactionFilters.js';
import {
  TRANSACTION_COLUMN_COUNT,
  TransactionsTable,
} from './transactions/TransactionsTable.js';

type Filters = NonNullable<ListTransactionsData['query']>;

const PAGE_SIZE = 20;

export default function TransactionsListPage() {
  const { state, cursor, canPrev, goNext, goPrev, setFilter } =
    useCursorPagination({ filterKeys: ['q', 'op'] });
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
  const { canNext, handleNext } = usePageHandlers(data?.page, goNext);

  const handleSearchChange = useCallback(
    (value: string) => setFilter('q', value || null),
    [setFilter]
  );
  const handleOperationTypeChange = useCallback(
    (value: string) => setFilter('op', value || null),
    [setFilter]
  );
  const handleClearFilters = useCallback(() => {
    setFilter('q', null);
    setFilter('op', null);
  }, [setFilter]);

  let body: ReactNode;
  if (isLoading) {
    body = (
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={10} columns={TRANSACTION_COLUMN_COUNT} />
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
        {hasFilters ? (
          <EmptyState
            icon={<SearchIcon />}
            title="No transactions match your filters"
            description="Try adjusting or clearing the active filters"
            action={
              <Button variant="contained" onClick={handleClearFilters}>
                Clear filters
              </Button>
            }
          />
        ) : (
          <TableEmptyState kind="transactions" />
        )}
      </Box>
    );
  } else {
    body = <TransactionsTable rows={rows} />;
  }

  return (
    <Stack spacing={3}>
      <Box>
        <Typography variant="heading3SemiBold" component="h1">
          Transactions list
        </Typography>
        <Typography variant="bodyRegular" sx={{ color: 'text.secondary' }}>
          All indexed transactions on the Stellar network
        </Typography>
      </Box>

      <Card>
        <TransactionFilters
          search={q}
          operationType={op}
          onSearchChange={handleSearchChange}
          onOperationTypeChange={handleOperationTypeChange}
        />
        <Box sx={{ minHeight: 320 }}>{body}</Box>
        <PaginationControls
          caption="All results"
          canPrev={canPrev}
          canNext={canNext}
          onPrev={goPrev}
          onNext={handleNext}
        />
      </Card>
    </Stack>
  );
}
