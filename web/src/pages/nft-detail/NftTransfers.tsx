import SwapHorizIcon from '@mui/icons-material/SwapHorizOutlined';
import { Box, Card, Typography } from '@mui/material';
import type { NftTransferItem } from '@rumblefish/api-types';
import {
  classifyError,
  EmptyState,
  ExplorerTable,
  GenericErrorState,
  IdentifierDisplay,
  PaginationControls,
  RateLimitState,
  TableSectionHeader,
  TableSkeleton,
  TransientErrorState,
  useCursorPagination,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import { useEffect, type ReactNode } from 'react';

import { useNftTransfers } from '../../api/index.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

import { NftEventBadge } from './NftEventBadge.js';

interface NftTransfersProps {
  /** Numeric `nfts.id` surrogate. */
  nftId: number;
}

function Dash() {
  return (
    <Typography component="span" sx={{ color: 'text.tertiary' }}>
      —
    </Typography>
  );
}

const columns: ExplorerTableColumn<NftTransferItem>[] = [
  {
    id: 'event',
    header: 'Event',
    cell: (row) => <NftEventBadge name={row.event_type_name} />,
  },
  {
    id: 'from',
    header: 'From',
    // `from_account` is null on the mint row.
    cell: (row) =>
      row.from_account ? (
        <IdentifierDisplay value={row.from_account} type="account" />
      ) : (
        <Dash />
      ),
  },
  {
    id: 'to',
    header: 'To',
    // `to_account` is null on a burn.
    cell: (row) =>
      row.to_account ? (
        <IdentifierDisplay value={row.to_account} type="account" />
      ) : (
        <Dash />
      ),
  },
  {
    id: 'transaction',
    header: 'Transaction',
    cell: (row) => (
      <IdentifierDisplay value={row.transaction_hash} type="transaction" />
    ),
  },
  {
    id: 'time',
    header: 'Time',
    cell: (row) => <TransactionTime createdAt={row.created_at} />,
  },
];

const COLUMN_COUNT = columns.length;

/**
 * Transfer-history section of the NFT detail page — a cursor-paginated table
 * of mint / transfer / burn events for one NFT, per the Figma design.
 */
export function NftTransfers({ nftId }: NftTransfersProps) {
  const { cursor, canPrev, goNext, goPrev, reset } = useCursorPagination();

  // Cursors are NFT-scoped; switching to a different NFT must drop the
  // URL cursor and stack. `reset` is intentionally absent from deps —
  // it's a fresh callback each render.
  useEffect(() => {
    reset();
  }, [nftId]);

  const { data, isLoading, isError, error, refetch } = useNftTransfers(
    nftId,
    cursor
  );

  const rows = data?.data ?? [];
  const nextCursor = data?.page.has_more ? data.page.cursor ?? null : null;
  const canNext = nextCursor !== null;

  const handleNext = () => {
    if (nextCursor) goNext(nextCursor);
  };

  let body: ReactNode;
  if (isLoading) {
    body = (
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={5} columns={COLUMN_COUNT} />
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
        <EmptyState
          icon={<SwapHorizIcon />}
          title="No transfer history"
          description="This NFT has no recorded mint, transfer or burn events."
        />
      </Box>
    );
  } else {
    body = (
      <ExplorerTable
        columns={columns}
        rows={rows}
        rowKey={(row) => `${row.transaction_hash}-${row.event_order}`}
      />
    );
  }

  return (
    <Card>
      <TableSectionHeader title="Transfer history" />
      <Box sx={{ minHeight: 200 }}>{body}</Box>
      <PaginationControls
        caption="Latest results"
        prevCursor={canPrev ? 'prev' : null}
        nextCursor={canNext ? 'next' : null}
        onPrev={goPrev}
        onNext={handleNext}
      />
    </Card>
  );
}
