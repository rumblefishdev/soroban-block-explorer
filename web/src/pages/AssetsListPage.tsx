import { Stack } from '@mui/material';
import type { ListAssetsData } from '@rumblefish/api-types';
import { useCursorPagination } from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';

import { PAGE_SIZE, useAssetsList, usePagedRows } from '../api/index.js';

import { AssetFilters } from './assets/AssetFilters.js';
import { AssetsTable } from './assets/AssetsTable.js';
import { codeIssuerRoute } from './assets/codeIssuerRoute.js';
import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';

type Filters = NonNullable<ListAssetsData['query']>;

export default function AssetsListPage() {
  const { state, cursor, goNext, goPrev, setFilter, setFilters, clearFilters } =
    useCursorPagination({
      filterKeys: ['code', 'type', 'sac'],
    });
  const code = state.filters.code ?? '';
  const type = state.filters.type ?? '';
  // SAC is a facet of classic/native, never Soroban (ADR 0051), so the two are
  // mutually exclusive: ignore a stale `sac` (e.g. from a pasted deep link)
  // while the Soroban type is active.
  const sac = state.filters.sac === 'true' && type !== 'soroban';
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

  const query = useAssetsList(cursor, queryFilters);
  const pager = usePagedRows(query.data, goNext, goPrev);

  const navigate = useNavigate();
  // A pasted `CODE:ISSUER` names one asset: open it rather than filter by a
  // needle no asset code can contain (task 0534).
  const handleSearchChange = useCallback(
    (value: string) => {
      const target = codeIssuerRoute(value);
      if (target) void navigate(target);
      else setFilter('code', value || null);
    },
    [navigate, setFilter]
  );
  // Soroban ⇄ "Has SAC" are mutually exclusive (ADR 0051). Each conflicting
  // transition is ONE atomic URL update (`setFilters`) so last-click wins and
  // the other chip clears — two `setFilter` calls would clobber each other
  // (see `useTableUrlState`).
  const handleTypeChange = useCallback(
    (value: string) =>
      value === 'soroban'
        ? setFilters({ type: 'soroban', sac: null })
        : setFilter('type', value || null),
    [setFilter, setFilters]
  );
  const handleSacChange = useCallback(
    (value: boolean) =>
      value && type === 'soroban'
        ? setFilters({ sac: 'true', type: null })
        : setFilter('sac', value ? 'true' : null),
    [setFilter, setFilters, type]
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
        query={query}
        pager={pager}
        renderTable={(rows, { loading }) => (
          <AssetsTable rows={rows} loading={loading} skeletonRows={PAGE_SIZE} />
        )}
        hasActiveFilters={hasFilters}
        emptyKind="tokens"
        emptyNoun="assets"
        onClearFilters={clearFilters}
      />
    </Stack>
  );
}
