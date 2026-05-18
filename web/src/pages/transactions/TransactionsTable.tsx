import type { TransactionListItem } from '@rumblefish/api-types';
import {
  Chip,
  ExplorerTable,
  IdentifierDisplay,
  IdentifierWithCopy,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import { Box, Typography } from '@mui/material';

import { formatFee } from './formatters.js';
import { formatOperationType } from './operationTypes.js';
import { TransactionTime } from './TransactionTime.js';

interface TransactionsTableProps {
  rows: readonly TransactionListItem[];
}

function Dash() {
  return (
    <Typography component="span" sx={{ color: 'text.tertiary' }}>
      —
    </Typography>
  );
}

function OperationCell({ types }: { types: readonly string[] }) {
  if (types.length === 0) return <Dash />;
  const [first, ...rest] = types;
  return (
    <Box sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.5 }}>
      <Chip size="sm" color="neutral" label={formatOperationType(first)} />
      {rest.length > 0 && (
        <Typography variant="bodyXsRegular" sx={{ color: 'text.tertiary' }}>
          +{rest.length}
        </Typography>
      )}
    </Box>
  );
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
    cell: (row) => (
      <Chip
        size="sm"
        color={row.successful ? 'success' : 'error'}
        dot
        label={row.successful ? 'Success' : 'Failed'}
      />
    ),
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
