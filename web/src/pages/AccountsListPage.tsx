import { Stack } from '@mui/material';
import {
  useCursorPagination,
  usePageHandlers,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo } from 'react';

import {
  useAccountsList,
  type AccountsListFilters,
  type AccountsSort,
} from '../api/hooks/useAccountsList.js';

import { AccountsFilters } from './accounts/AccountsFilters.js';
import {
  ACCOUNT_COLUMN_COUNT,
  AccountsTable,
} from './accounts/AccountsTable.js';
import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';

const PAGE_SIZE = 20;
const DEFAULT_SORT: AccountsSort = 'xlm_desc';
const SORT_VALUES = new Set<AccountsSort>([
  'xlm_desc',
  'last_seen_desc',
  'first_seen_desc',
]);

function parseSort(raw: string | undefined): AccountsSort {
  return raw && SORT_VALUES.has(raw as AccountsSort)
    ? (raw as AccountsSort)
    : DEFAULT_SORT;
}

const CAPTION_BY_SORT: Record<AccountsSort, string> = {
  xlm_desc: 'Top XLM holders',
  last_seen_desc: 'Recently active',
  first_seen_desc: 'New accounts',
};

export default function AccountsListPage() {
  const { state, cursor, goNext, goPrev, setFilter } = useCursorPagination({
    filterKeys: ['q', 'sort', 'domain'],
  });
  const q = state.filters.q ?? '';
  const sort = parseSort(state.filters.sort);
  const withDomain = state.filters.domain === '1';
  const hasFilters = q !== '' || withDomain;

  const filters = useMemo<AccountsListFilters>(
    () => ({
      limit: PAGE_SIZE,
      sort,
      ...(q ? { q } : {}),
      ...(withDomain ? { with_domain: true } : {}),
    }),
    [q, sort, withDomain]
  );

  const { data, isLoading, isError, error, refetch } = useAccountsList(
    cursor,
    filters
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
  const handleSortChange = useCallback(
    (value: AccountsSort) =>
      setFilter('sort', value === DEFAULT_SORT ? null : value),
    [setFilter]
  );
  const handleDomainChange = useCallback(
    (value: boolean) => setFilter('domain', value ? '1' : null),
    [setFilter]
  );
  const handleClearFilters = useCallback(() => {
    setFilter('q', null);
    setFilter('domain', null);
  }, [setFilter]);

  const startRank = Number(cursor) || 0;

  return (
    <Stack spacing={3}>
      <PageHeader
        title="Accounts"
        subtitle="All indexed Stellar accounts on the network"
      />
      <DataListCard
        filters={
          <AccountsFilters
            search={q}
            sort={sort}
            withDomain={withDomain}
            onSearchChange={handleSearchChange}
            onSortChange={handleSortChange}
            onWithDomainChange={handleDomainChange}
          />
        }
        columnCount={ACCOUNT_COLUMN_COUNT}
        isLoading={isLoading}
        isError={isError}
        error={error}
        onRetry={() => void refetch()}
        rows={rows}
        renderTable={(visibleRows) => (
          <AccountsTable rows={visibleRows} startRank={startRank} />
        )}
        hasActiveFilters={hasFilters}
        emptyKind="accounts"
        emptyNoun="accounts"
        onClearFilters={handleClearFilters}
        paginationCaption={CAPTION_BY_SORT[sort]}
        canPrev={canPrev}
        canNext={canNext}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </Stack>
  );
}
