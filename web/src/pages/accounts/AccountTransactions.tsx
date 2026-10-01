import { Typography } from '@mui/material';
import type { AccountTransactionItem } from '@rumblefish/api-types';
import {
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  formatFee,
  type SortDirection,
  useCursorPagination,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import { useCallback } from 'react';

import { useAccountTransactions, usePagedRows } from '../../api/index.js';
import { DataList } from '../detail/DataList.js';
import { SectionCard } from '../detail/SectionCard.js';
import { BalanceChangeCell } from './BalanceChangeCell.js';
import {
  hashColumn,
  ledgerColumn,
  OperationCell,
  statusColumn,
} from '../transactions/cells.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

const columns: ExplorerTableColumn<AccountTransactionItem>[] = [
  hashColumn<AccountTransactionItem>(),
  ledgerColumn<AccountTransactionItem>(),
  {
    id: 'operation',
    header: 'Operation',
    width: 190,
    cell: (row) => <OperationCell types={row.operation_types} />,
  },
  statusColumn<AccountTransactionItem>(),
  {
    // Account-relative, so it exists on THIS table only — a balance change
    // means nothing without an account in context (task 0540 / T07).
    id: 'balance_change',
    header: 'Balance change',
    align: 'right',
    width: 200,
    cell: (row) => <BalanceChangeCell changes={row.balance_changes} />,
  },
  {
    id: 'fee',
    header: 'Fee',
    align: 'right',
    width: 140,
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
    width: 210,
    cell: (row) => <TransactionTime createdAt={row.created_at} />,
  },
];

/**
 * Recent transactions section of the account detail page — a paginated table
 * of transactions involving the account, fetched independently of the
 * account summary so a failure here never collapses the rest of the page.
 */
export function AccountTransactions({ accountId }: { accountId: string }) {
  // Cursors are account-scoped — `resetKey` drops the URL cursor when
  // the user navigates to a different account.
  const { state, cursor, goNext, goPrev, setSort } = useCursorPagination({
    resetKey: accountId,
  });
  // Sort lives in the URL `sort` (column) + `dir` (direction) params via
  // `setSort`, so it survives reload / deep links and stays paired with
  // the cursor.
  const sortDir = state.sortDir;

  const handleSortChange = useCallback(
    // Column id comes from the table; `setSort` writes `?sort=&dir=` and
    // resets the cursor.
    (id: string, next: SortDirection) => setSort(id, next),
    [setSort]
  );

  const query = useAccountTransactions(accountId, cursor, sortDir);
  const pager = usePagedRows(query.data, goNext, goPrev);

  return (
    <SectionCard title="Recent transactions">
      <DataList
        query={query}
        pager={pager}
        errorPy={6}
        renderTable={(rows, { loading }) => (
          <ExplorerTable
            columns={columns}
            rows={rows}
            rowKey={(row) => row.hash}
            loading={loading}
            skeletonRows={20}
            rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
            // As before, the skeleton's header neither shows nor changes the sort.
            {...(loading
              ? {}
              : { sortBy: 'time', sortDir, onSortChange: handleSortChange })}
          />
        )}
        emptyKind="transactions"
        emptyPy={6}
      />
    </SectionCard>
  );
}
