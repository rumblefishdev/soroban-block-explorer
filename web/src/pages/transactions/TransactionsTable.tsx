import type { TransactionListItem } from '@rumblefish/api-types';
import {
  Dash,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  formatFee,
  IdentifierDisplay,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import { Typography } from '@mui/material';

import {
  hashColumn,
  ledgerColumn,
  OperationCell,
  statusColumn,
} from './cells.js';
import { TransactionTime } from './TransactionTime.js';

interface TransactionsTableProps {
  rows: readonly TransactionListItem[];
  loading?: boolean;
  skeletonRows?: number;
}

const columns: ExplorerTableColumn<TransactionListItem>[] = [
  hashColumn<TransactionListItem>(),
  ledgerColumn<TransactionListItem>(),
  {
    id: 'source',
    header: 'Source account',
    width: 160,
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
    width: 190,
    cell: (row) => <OperationCell types={row.operation_types} />,
  },
  statusColumn<TransactionListItem>(),
  // The "Net settled" column is built (`ValueCell` in ./cells.tsx) but NOT
  // rendered: the value it shows only exists once the prod rollout in task 0419
  // lands — the CH column, the indexer that writes it, and the full S3
  // re-ingest that materialises history. Until then the API returns no
  // `values` at all and every cell would read as a dash. Task 0411 owns when
  // and where this column comes back; it is also gated on 0417 (the read is a
  // partition scan on the polled global tx list).
  {
    id: 'fee',
    header: 'Fee',
    width: 140,
    cell: (row) => (
      <Typography component="span" variant="bodySmRegular">
        {formatFee(row.fee_charged)}
      </Typography>
    ),
  },
  {
    id: 'time',
    header: 'Time',
    width: 210,
    cell: (row) => <TransactionTime createdAt={row.created_at} />,
  },
];

/** Number of columns — used to size the loading skeleton consistently. */
export const TRANSACTION_COLUMN_COUNT = columns.length;

/**
 * The Transactions list table — hash, ledger, source account, operation,
 * status, fee and time columns, per the Figma design.
 */
export function TransactionsTable({
  rows,
  loading,
  skeletonRows,
}: TransactionsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => row.hash}
      rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      loading={loading}
      skeletonRows={skeletonRows}
    />
  );
}
