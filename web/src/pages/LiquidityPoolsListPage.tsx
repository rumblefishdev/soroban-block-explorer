import SearchIcon from '@mui/icons-material/SearchOutlined';
import { Box, Button, Card, Stack, Typography } from '@mui/material';
import type { ListPoolsData } from '@rumblefish/api-types';
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

import { usePoolsList } from '../api/index.js';

import { PoolsFilterBar } from './liquidity-pools/PoolsFilterBar.js';
import { POOL_COLUMN_COUNT, PoolsTable } from './liquidity-pools/PoolsTable.js';

type Filters = NonNullable<ListPoolsData['query']>;

const PAGE_SIZE = 20;

/**
 * Liquidity-pools list page (`/liquidity-pools`) — every liquidity pool
 * with asset-code search and a minimum-TVL preset filter. Cursor
 * paginated. Wires the Figma node `266:35969` layout against the
 * `GET /liquidity-pools` endpoint as extended by task 0246.
 */
export default function LiquidityPoolsListPage() {
  const { state, cursor, goNext, goPrev, setFilter } = useCursorPagination({
    filterKeys: ['asset', 'min_tvl'],
  });
  const asset = state.filters.asset ?? '';
  const minTvl = state.filters.min_tvl ?? '';
  const hasFilters = asset !== '' || minTvl !== '';

  const queryFilters = useMemo<Filters>(() => {
    const filters: Filters = { limit: PAGE_SIZE };
    if (asset) filters['filter[asset_code]'] = asset;
    if (minTvl) filters['filter[min_tvl]'] = minTvl;
    return filters;
  }, [asset, minTvl]);

  const { data, isLoading, isError, error, refetch } = usePoolsList(
    cursor,
    queryFilters
  );

  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );

  const handleAssetChange = useCallback(
    (value: string) => setFilter('asset', value || null),
    [setFilter]
  );
  const handleMinTvlChange = useCallback(
    (value: string) => setFilter('min_tvl', value || null),
    [setFilter]
  );
  const handleClearFilters = useCallback(() => {
    setFilter('asset', null);
    setFilter('min_tvl', null);
  }, [setFilter]);

  let body: ReactNode;
  if (isLoading) {
    body = (
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={10} columns={POOL_COLUMN_COUNT} />
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
            title="No pools match your filters"
            description="Try adjusting or clearing the active filters"
            action={
              <Button variant="contained" onClick={handleClearFilters}>
                Clear filters
              </Button>
            }
          />
        ) : (
          <TableEmptyState kind="pools" />
        )}
      </Box>
    );
  } else {
    body = <PoolsTable rows={rows} />;
  }

  return (
    <Stack spacing={3}>
      <Box>
        <Typography variant="heading3SemiBold" component="h1">
          Liquidity Pools
        </Typography>
        <Typography variant="bodyRegular" sx={{ color: 'text.secondary' }}>
          Liquidity pools on the Stellar network
        </Typography>
      </Box>

      <Card>
        <PoolsFilterBar
          asset={asset}
          minTvl={minTvl}
          onAssetChange={handleAssetChange}
          onMinTvlChange={handleMinTvlChange}
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
