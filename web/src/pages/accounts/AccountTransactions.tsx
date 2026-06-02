import { Box, Typography } from '@mui/material';
import type { AccountTransactionItem } from '@rumblefish/api-types';
import {
  ExplorerTable,
  formatFee,
  IdentifierDisplay,
  IdentifierWithCopy,
  PaginationControls,
  QueryErrorState,
  type SortDirection,
  StatusChip,
  TableEmptyState,
  TableSkeleton,
  useCursorPagination,
  usePageHandlers,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback, useState, type ReactNode } from 'react';

import { useAccountTransactions } from '../../api/index.js';
import { SectionCard } from '../detail/SectionCard.js';
import { OperationCell } from '../transactions/cells.js';
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
    cell: (row) => <StatusChip successful={row.successful} />,
  },
  {
    id: 'fee',
    header: 'Fee',
    align: 'right',
    cell: (row) => (
      <Typography
        component="span"
        variant="bodySmMedium"
        sx={(theme) => ({ color: theme.palette.text.primary })}
      >
        {formatFee(row.fee_charged)}
      </Typography>
    ),
  },
  {
    id: 'time',
    header: 'Time',
    sortable: true,
    cell: (row) => <TransactionTime createdAt={row.created_at} />,
  },
];

/**
 * Recent transactions section of the account detail page — a paginated table
 * of transactions involving the account, fetched independently of the
 * account summary so a failure here never collapses the rest of the page.
 */
export function AccountTransactions({ accountId }: { accountId: string }) {
  const [sortDir, setSortDir] = useState<SortDirection>('desc');
  // Cursors are account-scoped — `resetKey` drops the URL cursor when
  // the user navigates to a different account.
  const { cursor, goNext, goPrev, reset } = useCursorPagination({
    resetKey: accountId,
  });

  const handleSortChange = useCallback(
    (_id: string, next: SortDirection) => {
      setSortDir(next);

      reset();
    },
    [reset]
  );

  const { data, isLoading, isError, error, refetch } = useAccountTransactions(
    accountId,
    cursor,
    sortDir
  );

  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );

  let body: ReactNode;
  if (isLoading) {
    body = (
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={8} columns={columns.length} />
      </Box>
    );
  } else if (isError) {
    body = <QueryErrorState error={error} onRetry={() => void refetch()} />;
  } else if (rows.length === 0) {
    body = <TableEmptyState kind="transactions" py={6} />;
  } else {
    body = (
      <ExplorerTable
        columns={columns}
        rows={rows}
        rowKey={(row) => row.hash}
        sortBy="time"
        sortDir={sortDir}
        onSortChange={handleSortChange}
      />
    );
  }

  return (
    <SectionCard title="Recent transactions">
      <Box sx={{ minHeight: 280 }}>{body}</Box>
      <PaginationControls
        caption="Latest results"
        canPrev={canPrev}
        canNext={canNext}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </SectionCard>
  );
}
