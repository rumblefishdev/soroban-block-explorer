import SwapHorizIcon from '@mui/icons-material/SwapHorizOutlined';
import { Box, Card } from '@mui/material';
import type { NftTransferItem } from '@rumblefish/api-types';
import {
  Dash,
  EmptyState,
  ExplorerTable,
  IdentifierDisplay,
  PaginationControls,
  QueryErrorState,
  TableSectionHeader,
  TableSkeleton,
  useCursorPagination,
  usePageHandlers,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { useNftTransfers } from '../../api/index.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

import { NftEventBadge } from './NftEventBadge.js';

interface NftTransfersProps {
  /** Issuing contract C-strkey. */
  contractId: string;
  /** Opaque contract-defined token id (≤128 ASCII). */
  tokenId: string;
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
export function NftTransfers({ contractId, tokenId }: NftTransfersProps) {
  // Cursors are NFT-scoped — drop the URL cursor on NFT switch.
  const { cursor, goNext, goPrev } = useCursorPagination({
    resetKey: `${contractId}/${tokenId}`,
  });

  const { data, isLoading, isError, error, refetch } = useNftTransfers(
    contractId,
    tokenId,
    cursor
  );

  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );

  let body: ReactNode;
  if (isLoading) {
    body = <TableSkeleton rows={5} columns={COLUMN_COUNT} />;
  } else if (isError) {
    body = (
      <QueryErrorState error={error} onRetry={() => void refetch()} py={8} />
    );
  } else if (rows.length === 0) {
    body = (
      <EmptyState
        icon={<SwapHorizIcon />}
        title="No transfer history"
        description="This NFT has no recorded mint, transfer or burn events."
        py={8}
      />
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
        canPrev={canPrev}
        canNext={canNext}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </Card>
  );
}
