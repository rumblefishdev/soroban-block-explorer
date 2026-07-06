import { Stack } from '@mui/material';
import type { ListContractsData } from '@rumblefish/api-types';
import { useCursorPagination } from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo } from 'react';

import { PAGE_SIZE, useContractsList, usePagedRows } from '../api/index.js';

import { ContractsFilters } from './contracts/ContractsFilters.js';
import {
  CONTRACT_COLUMN_COUNT,
  ContractsTable,
} from './contracts/ContractsTable.js';
import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';

type Filters = NonNullable<ListContractsData['query']>;

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

  const { data, isLoading, isPlaceholderData, isError, error, refetch } =
    useContractsList(cursor, queryFilters);

  const { rows, canPrev, canNext, handlePrev, handleNext } = usePagedRows(
    data,
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
        isReloading={isPlaceholderData}
        isError={isError}
        error={error}
        onRetry={() => void refetch()}
        rows={rows}
        renderTable={(visibleRows) => <ContractsTable rows={visibleRows} />}
        renderSkeleton={() => (
          <ContractsTable rows={[]} loading skeletonRows={PAGE_SIZE} />
        )}
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
