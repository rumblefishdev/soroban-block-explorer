import { Stack } from '@mui/material';
import type { ListContractsData } from '@rumblefish/api-types';
import {
  useCursorPagination,
  usePageHandlers,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo } from 'react';

import { useContractsList } from '../api/index.js';

import { ContractsFilters } from './contracts/ContractsFilters.js';
import {
  CONTRACT_COLUMN_COUNT,
  ContractsTable,
} from './contracts/ContractsTable.js';
import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';

type Filters = NonNullable<ListContractsData['query']>;

const PAGE_SIZE = 20;

export default function ContractsListPage() {
  const { state, cursor, goNext, goPrev, setFilter, clearFilters } =
    useCursorPagination({
      filterKeys: ['q', 'type'],
    });
  const q = state.filters.q ?? '';
  const type = state.filters.type ?? '';
  const hasFilters = q !== '' || type !== '';

  const queryFilters = useMemo<Filters>(() => {
    const filters: Filters = { limit: PAGE_SIZE };
    if (q) filters['filter[q]'] = q;
    if (type) filters['filter[type]'] = type;
    return filters;
  }, [q, type]);

  const { data, isLoading, isError, error, refetch } = useContractsList(
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
  const handleTypeChange = useCallback(
    (value: string) => setFilter('type', value || null),
    [setFilter]
  );

  return (
    <Stack spacing={3}>
      <PageHeader
        title="Contracts"
        subtitle="All Soroban smart contracts on the Stellar network"
      />
      <DataListCard
        filters={
          <ContractsFilters
            search={q}
            type={type}
            onSearchChange={handleSearchChange}
            onTypeChange={handleTypeChange}
          />
        }
        columnCount={CONTRACT_COLUMN_COUNT}
        isLoading={isLoading}
        isError={isError}
        error={error}
        onRetry={() => void refetch()}
        rows={rows}
        renderTable={(visibleRows) => <ContractsTable rows={visibleRows} />}
        hasActiveFilters={hasFilters}
        emptyKind="contracts"
        emptyNoun="contracts"
        onClearFilters={clearFilters}
        canPrev={canPrev}
        canNext={canNext}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </Stack>
  );
}
