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
  useCursorPagination,
  usePageHandlers,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo, type ReactNode } from 'react';

import { useNftsList } from '../api/index.js';

import { NftFilters } from './nfts/NftFilters.js';
import { NFT_COLUMN_COUNT, NftsTable } from './nfts/NftsTable.js';

type Filters = NonNullable<ListNftsData['query']>;

const PAGE_SIZE = 20;

export default function NftsListPage() {
  const { state, cursor, canPrev, goNext, goPrev, setFilter } =
    useCursorPagination({ filterKeys: ['collection', 'contract'] });
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

  const { data, isLoading, isError, error, refetch } = useNftsList(
    cursor,
    queryFilters
  );

  const rows = data?.data ?? [];
  const { canNext, handleNext } = usePageHandlers(data?.page, goNext);

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
          canPrev={canPrev}
          canNext={canNext}
          onPrev={goPrev}
          onNext={handleNext}
        />
      </Card>
    </Stack>
  );
}
