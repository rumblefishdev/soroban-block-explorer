import { Box, Skeleton, Stack } from '@mui/material';
import type { NftItem } from '@rumblefish/api-types';
import {
  IdentifierDisplay,
  PaginationControls,
  QueryErrorState,
  TableEmptyState,
  useCursorPagination,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { PAGE_SIZE, useNftsList, usePagedRows } from '../../api/index.js';
import { routes } from '../../router/routes.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { NftNameCell } from '../nfts/NftNameCell.js';

/** One NFT, laid out like a row of the Tokens list beside it. */
function NftRow({ row, alt }: { row: NftItem; alt: boolean }) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 2,
        px: 2,
        py: 1,
        backgroundColor: alt
          ? theme.palette.surface.grayMainAlt
          : theme.palette.surface.grayMain,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        '&:last-of-type': { borderBottom: 'none' },
      })}
    >
      <NftNameCell row={row} nameFontSize={16} mono={false} />
      {/* The collection, linked to its NFT list. A row has no column header,
          so a bare contract id here does not say what it is: the name goes
          first, the short id only when the collection has none. */}
      <IdentifierDisplay
        value={row.collection_name ?? row.contract_id}
        type="contract"
        href={routes.nftsByContract(row.contract_id)}
        truncate={!row.collection_name}
        mono={!row.collection_name}
      />
    </Box>
  );
}

/** A loading row the height of `NftRow`, so the card does not jump. */
function NftRowPlaceholder({ alt }: { alt: boolean }) {
  return (
    <Box
      data-testid="nft-row-placeholder"
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        gap: 1,
        px: 2,
        py: 1,
        backgroundColor: alt
          ? theme.palette.surface.grayMainAlt
          : theme.palette.surface.grayMain,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        '&:last-of-type': { borderBottom: 'none' },
      })}
    >
      <Skeleton variant="rounded" width={40} height={40} />
      <Stack>
        <Skeleton width={120} height={22} />
        <Skeleton width={32} height={17} />
      </Stack>
      <Skeleton width={76} height={20} sx={{ ml: 'auto' }} />
    </Box>
  );
}

/**
 * The NFTs view of the account's Assets card — the NFTs the account holds now,
 * each linked to its NFT page, newest mint first.
 */
export function AccountNfts({
  accountId,
  nftCount,
}: {
  accountId: string;
  /** NFTs the account holds now (`nft_count`). */
  nftCount: number;
}) {
  // Own URL key: the transactions section below pages with `?cursor=`.
  const { cursor, goNext, goPrev } = useCursorPagination({
    cursorParam: CURSOR_PARAMS.ACCOUNT_NFTS,
    resetKey: accountId,
  });
  const query = useNftsList(cursor, {
    limit: PAGE_SIZE,
    'filter[owner]': accountId,
  });
  const pager = usePagedRows(query.data, goNext, goPrev);

  let body: ReactNode;
  if (query.isLoading || query.isPlaceholderData) {
    // As many rows as the page will hold, so the rows replace them in place.
    body = Array.from({ length: Math.min(nftCount, PAGE_SIZE) }, (_, i) => (
      <NftRowPlaceholder key={i} alt={i % 2 === 1} />
    ));
  } else if (query.isError) {
    body = (
      <QueryErrorState
        error={query.error}
        onRetry={() => void query.refetch()}
        py={6}
      />
    );
  } else if (pager.rows.length === 0) {
    body = <TableEmptyState kind="nft" py={6} />;
  } else {
    body = pager.rows.map((row, index) => (
      <NftRow
        key={`${row.contract_id}:${row.token_id}`}
        row={row}
        alt={index % 2 === 1}
      />
    ));
  }

  return (
    <>
      {body}
      {/* Like the Tokens list: a pager only when there is a second page. The
          cursor does not know which page it is on, so the caption gives the
          total instead of a position. */}
      {nftCount > PAGE_SIZE && (
        <PaginationControls
          caption={`${nftCount} NFTs`}
          canPrev={pager.canPrev}
          canNext={pager.canNext}
          onPrev={pager.handlePrev}
          onNext={pager.handleNext}
        />
      )}
    </>
  );
}
