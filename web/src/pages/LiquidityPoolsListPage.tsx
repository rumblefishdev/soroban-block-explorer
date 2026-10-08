import { Stack } from '@mui/material';
import type { ListPoolsData } from '@rumblefish/api-types';
import { useCursorPagination } from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';

import { PAGE_SIZE, usePoolsList, usePagedRows } from '../api/index.js';

import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';
import { PoolSearchPrototype } from './liquidity-pools/PoolSearchPrototype.js';
import { PoolsFilterBar } from './liquidity-pools/PoolsFilterBar.js';
import { PoolsTable } from './liquidity-pools/PoolsTable.js';

type Filters = NonNullable<ListPoolsData['query']>;

/**
 * Liquidity-pools list page (`/liquidity-pools`) — every liquidity pool,
 * classic and Soroban alike, with asset-code search, a pool-kind filter and a
 * minimum-TVL preset filter. Cursor paginated. Wires the Figma node
 * `266:35969` layout against the `GET /liquidity-pools` endpoint as extended
 * by task 0246.
 */
export default function LiquidityPoolsListPage() {
  // PROTOTYPE (W341): ?variant=A|B|C swaps the list for the search prototype.
  // Dev only.
  const [protoParams] = useSearchParams();
  const protoVariant = protoParams.get('variant');
  if (import.meta.env.DEV && protoVariant) {
    return (
      <Stack spacing={3}>
        <PageHeader
          title="Liquidity Pools"
          subtitle="PROTOTYPE W341 — pool search variants (all production pools, local fixture)"
        />
        <PoolSearchPrototype variant={protoVariant} />
      </Stack>
    );
  }
  return <RealLiquidityPoolsListPage />;
}

function RealLiquidityPoolsListPage() {
  const { state, cursor, goNext, goPrev, setFilter, clearFilters } =
    useCursorPagination({
      filterKeys: ['asset', 'kind', 'min_tvl'],
    });
  const asset = state.filters.asset ?? '';
  const kind = state.filters.kind ?? '';
  const minTvl = state.filters.min_tvl ?? '';
  const hasFilters = asset !== '' || kind !== '' || minTvl !== '';

  const queryFilters = useMemo<Filters>(() => {
    const filters: Filters = { limit: PAGE_SIZE };
    if (asset) filters['filter[asset_code]'] = asset;
    if (kind) filters['filter[pool_kind]'] = kind;
    if (minTvl) filters['filter[min_tvl]'] = minTvl;
    return filters;
  }, [asset, kind, minTvl]);

  const query = usePoolsList(cursor, queryFilters);
  const pager = usePagedRows(query.data, goNext, goPrev);

  const handleAssetChange = useCallback(
    (value: string) => setFilter('asset', value || null),
    [setFilter]
  );
  const handleKindChange = useCallback(
    (value: string) => setFilter('kind', value || null),
    [setFilter]
  );
  const handleMinTvlChange = useCallback(
    (value: string) => setFilter('min_tvl', value || null),
    [setFilter]
  );

  return (
    <Stack spacing={3}>
      <PageHeader
        title="Liquidity Pools"
        subtitle="All AMM liquidity pools on the Stellar network"
      />
      <DataListCard
        filters={
          <PoolsFilterBar
            asset={asset}
            kind={kind}
            minTvl={minTvl}
            onAssetChange={handleAssetChange}
            onKindChange={handleKindChange}
            onMinTvlChange={handleMinTvlChange}
          />
        }
        query={query}
        pager={pager}
        renderTable={(rows, { loading }) => (
          <PoolsTable rows={rows} loading={loading} skeletonRows={PAGE_SIZE} />
        )}
        hasActiveFilters={hasFilters}
        emptyKind="pools"
        emptyNoun="pools"
        onClearFilters={clearFilters}
      />
    </Stack>
  );
}
