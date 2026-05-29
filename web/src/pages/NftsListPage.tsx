import { Stack } from '@mui/material';
import type { ListNftsData } from '@rumblefish/api-types';
import {
  isContractId,
  useCursorPagination,
  usePageHandlers,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useMemo } from 'react';

import { useNftsList } from '../api/index.js';

import { DataListCard } from './detail/DataListCard.js';
import { PageHeader } from './detail/PageHeader.js';
import { NftFilters } from './nfts/NftFilters.js';
import { NFT_COLUMN_COUNT, NftsTable } from './nfts/NftsTable.js';

type Filters = NonNullable<ListNftsData['query']>;

const PAGE_SIZE = 20;

export default function NftsListPage() {
  const { state, cursor, goNext, goPrev, setFilter } = useCursorPagination({
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

  const { data, isLoading, isError, error, refetch } = useNftsList(
    cursor,
    queryFilters
  );

  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );

  const handleClearFilters = useCallback(() => {
    setFilter('collection', null);
    setFilter('contract', null);
  }, [setFilter]);

  return (
    <Stack spacing={3}>
      <PageHeader
        title="NFTs"
        subtitle="Soroban-based NFT contracts on the Stellar network"
      />
      <DataListCard
        filters={
          <NftFilters
            collection={collection}
            contractId={contract}
            onCollectionChange={(v) => setFilter('collection', v || null)}
            onContractIdChange={(v) => setFilter('contract', v || null)}
          />
        }
        columnCount={NFT_COLUMN_COUNT}
        isLoading={isLoading}
        isError={isError}
        error={error}
        onRetry={() => void refetch()}
        rows={rows}
        renderTable={(visibleRows) => <NftsTable rows={visibleRows} />}
        hasActiveFilters={hasFilters}
        emptyKind="nft"
        emptyNoun="NFTs"
        onClearFilters={handleClearFilters}
        canPrev={canPrev}
        canNext={canNext}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </Stack>
  );
}
