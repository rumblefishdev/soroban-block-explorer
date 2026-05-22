import { Box, Typography } from '@mui/material';
import type { AccountTransactionItem } from '@rumblefish/api-types';
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
  useCursorPagination,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import { useEffect, type ReactNode } from 'react';

import { useAccountTransactions } from '../../api/index.js';
import { SectionCard } from '../detail/SectionCard.js';
import { OperationCell, StatusCell } from '../transactions/cells.js';
import { formatFee } from '../transactions/formatters.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

const columns: ExplorerTableColumn<AccountTransactionItem>[] = [
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
    id: 'fee',
    header: 'Fee',
    align: 'right',
    cell: (row) => (
      <Typography component="span" variant="bodySmRegular">
        {formatFee(row.fee_charged)}
      </Typography>
    ),
  },
  {
    id: 'time',
    header: 'Time',
    cell: (row) => <TransactionTime createdAt={row.created_at} />,
  },
];

/**
 * Recent transactions section of the account detail page — a paginated table
 * of transactions involving the account, fetched independently of the
 * account summary so a failure here never collapses the rest of the page.
 */
export function AccountTransactions({ accountId }: { accountId: string }) {
  const { cursor, canPrev, goNext, goPrev, reset } = useCursorPagination();

  // Cursors are account-scoped; switching accounts must drop the URL
  // cursor. `reset` is intentionally absent from deps — fresh callback
  // each render.
  useEffect(() => {
    reset();
  }, [accountId]);

  const { data, isLoading, isError, error, refetch } = useAccountTransactions(
    accountId,
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
    <SectionCard title="Recent transactions">
      <Box sx={{ minHeight: 280 }}>{body}</Box>
      <PaginationControls
        caption="Latest results"
        prevCursor={canPrev ? 'prev' : null}
        nextCursor={canNext ? 'next' : null}
        onPrev={goPrev}
        onNext={handleNext}
      />
    </SectionCard>
  );
}
