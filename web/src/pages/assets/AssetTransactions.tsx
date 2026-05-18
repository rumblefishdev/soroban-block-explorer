import { Box } from '@mui/material';
import type { AssetTransactionItem } from '@rumblefish/api-types';
import {
  classifyError,
  ExplorerTable,
  GenericErrorState,
  IdentifierDisplay,
  IdentifierWithCopy,
  PaginationControls,
  RateLimitState,
  TableEmptyState,
  TableSkeleton,
  TransientErrorState,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { useAssetTransactions } from '../../api/index.js';
import { SectionCard } from '../detail/SectionCard.js';
import { Dash, OperationCell, StatusCell } from '../transactions/cells.js';
import { TransactionTime } from '../transactions/TransactionTime.js';
import { useInfinitePager } from '../useInfinitePager.js';

const columns: ExplorerTableColumn<AssetTransactionItem>[] = [
  {
    id: 'hash',
    header: 'Hash',
    cell: (row) => <IdentifierWithCopy value={row.hash} type="transaction" />,
  },
  {
    id: 'ledger',
    header: 'Ledger',
    cell: (row) => (
      <IdentifierDisplay value={String(row.ledger_sequence)} type="ledger" />
    ),
  },
  {
    id: 'source',
    header: 'Source account',
    cell: (row) =>
      row.source_account ? (
        <IdentifierDisplay value={row.source_account} type="account" />
      ) : (
        <Dash />
      ),
  },
  {
    id: 'operation',
    header: 'Operation',
    cell: (row) => <OperationCell types={row.operation_types} />,
  },
  {
    id: 'status',
    header: 'Status',
    cell: (row) => <StatusCell successful={row.successful} />,
  },
  {
    id: 'time',
    header: 'Time',
    cell: (row) => <TransactionTime createdAt={row.created_at} />,
  },
];

/**
 * Latest transactions section of the asset detail page — a paginated table of
 * transactions involving the asset, fetched independently of the asset
 * summary and metadata.
 */
export function AssetTransactions({ assetId }: { assetId: string }) {
  const {
    data,
    isLoading,
    isError,
    error,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    refetch,
  } = useAssetTransactions(assetId);

  const { rows, canPrev, canNext, handlePrev, handleNext } = useInfinitePager(
    data?.pages ?? [],
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage
  );

  let body: ReactNode;
  if (isLoading) {
    body = (
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={8} columns={columns.length} />
      </Box>
    );
  } else if (isError) {
    const kind = classifyError(error);
    const retry = () => void refetch();
    body = (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 6 }}>
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
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 6 }}>
        <TableEmptyState kind="transactions" />
      </Box>
    );
  } else {
    body = (
      <ExplorerTable columns={columns} rows={rows} rowKey={(row) => row.hash} />
    );
  }

  return (
    <SectionCard title="Latest transactions">
      <Box sx={{ minHeight: 280 }}>{body}</Box>
      <PaginationControls
        caption="Latest results"
        prevCursor={canPrev ? 'prev' : null}
        nextCursor={canNext ? 'next' : null}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </SectionCard>
  );
}
