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
      filterKeys: ['code', 'type', 'sac'],
    });
  const code = state.filters.code ?? '';
  const type = state.filters.type ?? '';
  const sac = state.filters.sac === 'true';
  const hasFilters = code !== '' || type !== '' || sac;

  const queryFilters = useMemo<Filters>(() => {
    const filters: Filters = { limit: PAGE_SIZE };
    if (code) filters['filter[code]'] = code;
    if (type) filters['filter[type]'] = type;
    // ADR 0051: "has SAC" is a property filter orthogonal to the asset type —
    // it restricts to rows carrying a deployed SAC facet (`filter[sac]=true`).
    if (sac) filters['filter[sac]'] = 'true';
    return filters;
  }, [code, type, sac]);

  const { data, isLoading, isPlaceholderData, isError, error, refetch } =
    useAssetsList(cursor, queryFilters);

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
  const handleSacChange = useCallback(
    (value: boolean) => setFilter('sac', value ? 'true' : null),
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
            sac={sac}
            onSearchChange={handleSearchChange}
            onTypeChange={handleTypeChange}
            onSacChange={handleSacChange}
          />
        }
        columnCount={ASSET_COLUMN_COUNT}
        isLoading={isLoading}
        isReloading={isPlaceholderData}
        isError={isError}
        error={error}
        onRetry={() => void refetch()}
        rows={rows}
        renderTable={(visibleRows) => <AssetsTable rows={visibleRows} />}
        renderSkeleton={() => (
          <AssetsTable rows={[]} loading skeletonRows={PAGE_SIZE} />
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
