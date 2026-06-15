import { Stack } from '@mui/material';
import type { ListAssetsData } from '@rumblefish/api-types';
import {
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
  const { state, cursor, goNext, goPrev, setFilter, clearFilters } =
    useCursorPagination({
      filterKeys: ['code', 'type'],
    });
  const code = state.filters.code ?? '';
  const type = state.filters.type ?? '';
  const hasFilters = code !== '' || type !== '';

  const queryFilters = useMemo<Filters>(() => {
    const filters: Filters = { limit: PAGE_SIZE };
    if (code) filters['filter[code]'] = code;
    if (type) filters['filter[type]'] = type;
    return filters;
  }, [code, type]);

  const { data, isLoading, isError, error, refetch } = useAssetsList(
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
        renderTable={(visibleRows) => <AssetsTable rows={visibleRows} />}
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
