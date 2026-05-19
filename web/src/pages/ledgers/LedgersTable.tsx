import type { LedgerListItem } from '@rumblefish/api-types';
import {
  ExplorerTable,
  IdentifierDisplay,
  IdentifierWithCopy,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import { Typography } from '@mui/material';

import { TransactionTime } from '../transactions/TransactionTime.js';

interface LedgersTableProps {
  rows: readonly LedgerListItem[];
}

const columns: ExplorerTableColumn<LedgerListItem>[] = [
  {
    id: 'sequence',
    header: 'Sequence',
    cell: (row) => (
      <IdentifierDisplay
        value={String(row.sequence)}
        type="ledger"
        truncate={false}
      />
    ),
  },
  {
    id: 'hash',
    header: 'Hash',
    // Middle-truncated, not linked — a ledger has no hash route, and the
    // `ledger` entity type only truncates sequence numbers.
    cell: (row) => (
      <IdentifierWithCopy
        value={row.hash}
        type="ledger"
        linked={false}
        truncation={{ prefix: 4, suffix: 4 }}
      />
    ),
  },
  {
    id: 'closed_at',
    header: 'Closed at',
    cell: (row) => <TransactionTime createdAt={row.closed_at} />,
  },
  {
    id: 'protocol',
    header: 'Protocol',
    // Plain number, no chip — per the Figma ledger table.
    cell: (row) => (
      <Typography component="span" variant="bodySmRegular">
        {row.protocol_version}
      </Typography>
    ),
  },
  {
    id: 'tx_count',
    header: 'TX Count',
    align: 'right',
    cell: (row) => (
      <Typography component="span" variant="bodySmRegular">
        {row.transaction_count.toLocaleString('en-US')}
      </Typography>
    ),
  },
];

/** Column count — used to size the loading skeleton consistently. */
export const LEDGER_COLUMN_COUNT = columns.length;

/**
 * The Ledgers list table — sequence, hash, closed-at, protocol and
 * transaction-count columns, per the Figma design.
 */
export function LedgersTable({ rows }: LedgersTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => String(row.sequence)}
    />
  );
}
