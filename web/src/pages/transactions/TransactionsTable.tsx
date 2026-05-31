import type { TransactionListItem } from '@rumblefish/api-types';
import {
  Dash,
  ExplorerTable,
  formatFee,
  IdentifierDisplay,
  IdentifierWithCopy,
  StatusChip,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import { Typography } from '@mui/material';

import { OperationCell } from './cells.js';
import { TransactionTime } from './TransactionTime.js';

interface TransactionsTableProps {
  rows: readonly TransactionListItem[];
}

const columns: ExplorerTableColumn<TransactionListItem>[] = [
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
    cell: (row) => <StatusChip successful={row.successful} />,
  },
  {
    id: 'fee',
    header: 'Fee',
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

/** Number of columns — used to size the loading skeleton consistently. */
export const TRANSACTION_COLUMN_COUNT = columns.length;

/**
 * The Transactions list table — hash, ledger, source account, operation,
 * status, fee and time columns, per the Figma design.
 */
export function TransactionsTable({ rows }: TransactionsTableProps) {
  return (
    <ExplorerTable columns={columns} rows={rows} rowKey={(row) => row.hash} />
  );
}
