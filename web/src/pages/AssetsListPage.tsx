import { Stack } from '@mui/material';
import type { ListAssetsData } from '@rumblefish/api-types';
import {
  type SortDirection,
  useCursorPagination,
  usePageHandlers,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo } from 'react';

import { useAssetsList } from '../api/index.js';

import { AssetFilters } from './assets/AssetFilters.js';
import { ASSET_COLUMN_COUNT, AssetsTable } from './assets/AssetsTable.js';
import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';

type Filters = NonNullable<ListAssetsData['query']>;

const PAGE_SIZE = 20;

export default function AssetsListPage() {
  const { state, cursor, goNext, goPrev, setFilter, setSort, clearFilters } =
    useCursorPagination({
      filterKeys: ['code', 'type'],
    });
  const code = state.filters.code ?? '';
  const type = state.filters.type ?? '';
  const hasFilters = code !== '' || type !== '';
  // Sort lives in the URL `sort` (column) + `dir` (direction) params via
  // `setSort`, not local state — so it survives reload / deep links and
  // stays paired with the cursor it was generated under.
  const sortDir = state.sortDir;

  const queryFilters = useMemo<Filters>(() => {
    const filters: Filters = { limit: PAGE_SIZE };
    if (code) filters['filter[code]'] = code;
    if (type) filters['filter[type]'] = type;
    return filters;
  }, [code, type]);

  const { data, isLoading, isError, error, refetch } = useAssetsList(
    cursor,
    queryFilters,
    sortDir
  );

  const handleSortChange = useCallback(
    // Column id comes from the table; `setSort` writes `?sort=&dir=` and
    // resets the cursor (new ordering = page 1).
    (id: string, next: SortDirection) => setSort(id, next),
    [setSort]
  );

  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );

  const handleSearchChange = useCallback(
    (value: string) => setFilter('code', value || null),
    [setFilter]
  );
  const handleTypeChange = useCallback(
    (value: string) => setFilter('type', value || null),
    [setFilter]
  );

  return (
    <Stack spacing={3}>
      <PageHeader
        title="Assets"
        subtitle="All classic assets and Soroban token contracts on the Stellar network"
      />
      <DataListCard
        filters={
          <AssetFilters
            search={code}
            type={type}
            onSearchChange={handleSearchChange}
            onTypeChange={handleTypeChange}
          />
        }
        columnCount={ASSET_COLUMN_COUNT}
        isLoading={isLoading}
        isError={isError}
        error={error}
        onRetry={() => void refetch()}
        rows={rows}
        renderTable={(visibleRows) => (
          <AssetsTable
            rows={visibleRows}
            sortDir={sortDir}
            onSortChange={handleSortChange}
          />
        )}
        hasActiveFilters={hasFilters}
        emptyKind="tokens"
        emptyNoun="assets"
        onClearFilters={clearFilters}
        canPrev={canPrev}
        canNext={canNext}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </Stack>
  );
}
