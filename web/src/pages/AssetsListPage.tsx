import SearchIcon from '@mui/icons-material/SearchOutlined';
import { Box, Button, Card, Stack, Typography } from '@mui/material';
import type { ListAssetsData } from '@rumblefish/api-types';
import {
  classifyError,
  EmptyState,
  GenericErrorState,
  PaginationControls,
  RateLimitState,
  TableEmptyState,
  TableSkeleton,
  TransientErrorState,
  useCursorPagination,
  usePageHandlers,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo, type ReactNode } from 'react';

import { useAssetsList } from '../api/index.js';

import { AssetFilters } from './assets/AssetFilters.js';
import { ASSET_COLUMN_COUNT, AssetsTable } from './assets/AssetsTable.js';

type Filters = NonNullable<ListAssetsData['query']>;

const PAGE_SIZE = 20;

/**
 * Assets list page (`/assets`) — every classic asset and Soroban token
 * contract, with asset-code search and an asset-type filter. Cursor paginated.
 */
export default function AssetsListPage() {
  const { state, cursor, goNext, goPrev, setFilter } = useCursorPagination({
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
  const handleClearFilters = useCallback(() => {
    setFilter('code', null);
    setFilter('type', null);
  }, [setFilter]);

  let body: ReactNode;
  if (isLoading) {
    body = (
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={10} columns={ASSET_COLUMN_COUNT} />
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
            title="No assets match your filters"
            description="Try adjusting or clearing the active filters"
            action={
              <Button variant="contained" onClick={handleClearFilters}>
                Clear filters
              </Button>
            }
          />
        ) : (
          <TableEmptyState kind="tokens" />
        )}
      </Box>
    );
  } else {
    body = <AssetsTable rows={rows} />;
  }

  return (
    <Stack spacing={3}>
      <Box>
        <Typography variant="heading3SemiBold" component="h1">
          Assets
        </Typography>
        <Typography variant="bodyRegular" sx={{ color: 'text.secondary' }}>
          All classic assets and Soroban token contracts on the Stellar network
        </Typography>
      </Box>

      <Card>
        <AssetFilters
          search={code}
          type={type}
          onSearchChange={handleSearchChange}
          onTypeChange={handleTypeChange}
        />
        <Box sx={{ minHeight: 320 }}>{body}</Box>
        <PaginationControls
          caption="Latest results"
          canPrev={canPrev}
          canNext={canNext}
          onPrev={handlePrev}
          onNext={handleNext}
        />
      </Card>
    </Stack>
  );
}
