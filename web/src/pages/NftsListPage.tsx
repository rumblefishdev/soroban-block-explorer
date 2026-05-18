import SearchIcon from '@mui/icons-material/SearchOutlined';
import { Box, Button, Card, Stack, Typography } from '@mui/material';
import type { ListNftsData } from '@rumblefish/api-types';
import {
  classifyError,
  EmptyState,
  GenericErrorState,
  isContractId,
  PaginationControls,
  RateLimitState,
  TableEmptyState,
  TableSkeleton,
  TransientErrorState,
  useTableUrlState,
} from '@rumblefish/soroban-block-explorer-ui';
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

import { useNftsList } from '../api/index.js';

import { NftFilters } from './nfts/NftFilters.js';
import { NFT_COLUMN_COUNT, NftsTable } from './nfts/NftsTable.js';

type Filters = NonNullable<ListNftsData['query']>;

const PAGE_SIZE = 20;

export default function NftsListPage() {
  const { state, setFilter } = useTableUrlState({
    filterKeys: ['collection', 'contract'],
  });
  const collection = state.filters.collection ?? '';
  const contract = state.filters.contract ?? '';
  const hasFilters = collection !== '' || contract !== '';

  const queryFilters = useMemo<Filters>(() => {
    const filters: Filters = { limit: PAGE_SIZE };
    if (collection) filters['filter[collection]'] = collection;
    // Only a well-formed C-StrKey is a valid contract filter; anything else
    // the API would reject, so unrecognised input applies no filter.
    if (contract && isContractId(contract)) {
      filters['filter[contract_id]'] = contract;
    }
    return filters;
  }, [collection, contract]);

  const {
    data,
    isLoading,
    isError,
    error,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    refetch,
  } = useNftsList(queryFilters);

  const [pageIndex, setPageIndex] = useState(0);

  // A new filter set is a fresh query starting at page 0.
  useEffect(() => {
    setPageIndex(0);
  }, [collection, contract]);

  const pages = data?.pages ?? [];
  const currentPage = pages[pageIndex];
  const rows = currentPage?.data ?? [];
  const canPrev = pageIndex > 0;
  const canNext = Boolean(currentPage?.page.has_more);

  const handlePrev = useCallback(() => {
    setPageIndex((index) => Math.max(0, index - 1));
  }, []);

  const handleNext = useCallback(() => {
    if (isFetchingNextPage) return;
    if (pageIndex + 1 < pages.length) {
      setPageIndex(pageIndex + 1);
      return;
    }
    if (hasNextPage) {
      void fetchNextPage().then(() => setPageIndex((index) => index + 1));
    }
  }, [pageIndex, pages.length, hasNextPage, isFetchingNextPage, fetchNextPage]);

  const handleClearFilters = useCallback(() => {
    setFilter('collection', null);
    setFilter('contract', null);
  }, [setFilter]);

  let body: ReactNode;
  if (isLoading) {
    body = (
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={10} columns={NFT_COLUMN_COUNT} />
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
            title="No NFTs match your filters"
            description="Try adjusting or clearing the active filters"
            action={
              <Button variant="contained" onClick={handleClearFilters}>
                Clear filters
              </Button>
            }
          />
        ) : (
          <TableEmptyState kind="nft" />
        )}
      </Box>
    );
  } else {
    body = <NftsTable rows={rows} />;
  }

  return (
    <Stack spacing={3}>
      <Box>
        <Typography variant="heading3SemiBold" component="h1">
          NFTs
        </Typography>
        <Typography variant="bodyRegular" sx={{ color: 'text.secondary' }}>
          Soroban-based NFT contracts on the Stellar network
        </Typography>
      </Box>

      <Card>
        <NftFilters
          collection={collection}
          contractId={contract}
          onCollectionChange={(v) => setFilter('collection', v || null)}
          onContractIdChange={(v) => setFilter('contract', v || null)}
        />
        <Box sx={{ minHeight: 320 }}>{body}</Box>
        <PaginationControls
          caption="Latest results"
          prevCursor={canPrev ? 'prev' : null}
          nextCursor={canNext ? 'next' : null}
          onPrev={handlePrev}
          onNext={handleNext}
        />
      </Card>
    </Stack>
  );
}
