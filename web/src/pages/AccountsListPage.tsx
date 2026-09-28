import { Stack } from '@mui/material';
import type { ListAccountsData } from '@rumblefish/api-types';
import {
  type SortDirection,
  useCursorPagination,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo } from 'react';

import { PAGE_SIZE, useAccountsList, usePagedRows } from '../api/index.js';

import { AccountsFilters } from './accounts/AccountsFilters.js';
import { AccountsTable } from './accounts/AccountsTable.js';
import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';

type Filters = NonNullable<ListAccountsData['query']>;

export default function AccountsListPage() {
  const { state, cursor, goNext, goPrev, setFilter, setSort, clearFilters } =
    useCursorPagination({
      filterKeys: ['domain'],
    });
  const withDomain = state.filters.domain === '1';
  // Sort lives in the URL `dir` param (read via `state.sortDir`); the only
  // sortable column is `last_seen_ledger`. Default DESC = recently active.
  const sortDir = state.sortDir;
  const hasFilters = withDomain;

  const queryFilters = useMemo<Filters>(() => {
    const filters: Filters = { limit: PAGE_SIZE, order: sortDir };
    if (withDomain) filters['filter[with_domain]'] = true;
    return filters;
  }, [withDomain, sortDir]);

  const query = useAccountsList(cursor, queryFilters);
  const pager = usePagedRows(query.data, goNext, goPrev);

  const handleWithDomainChange = useCallback(
    (value: boolean) => setFilter('domain', value ? '1' : null),
    [setFilter]
  );
  const handleSortChange = useCallback(
    // Column id comes from the table; `setSort` writes `?sort=&dir=` and
    // resets the cursor.
    (id: string, next: SortDirection) => setSort(id, next),
    [setSort]
  );

  return (
    <Stack spacing={3}>
      <PageHeader
        title="Accounts"
        subtitle="All indexed Stellar accounts on the network"
      />
      <DataListCard
        filters={
          <AccountsFilters
            withDomain={withDomain}
            onWithDomainChange={handleWithDomainChange}
          />
        }
        query={query}
        pager={pager}
        renderTable={(rows, { loading }) => (
          <AccountsTable
            rows={rows}
            loading={loading}
            skeletonRows={PAGE_SIZE}
            // As before, the skeleton's header neither shows nor changes the sort.
            {...(loading ? {} : { sortDir, onSortChange: handleSortChange })}
          />
        )}
        hasActiveFilters={hasFilters}
        emptyKind="accounts"
        emptyNoun="accounts"
        onClearFilters={clearFilters}
      />
    </Stack>
  );
}
