import type { LedgerListItem } from '@rumblefish/api-types';
import {
  Chip,
  ExplorerTable,
  formatInteger,
  IdentifierDisplay,
  IdentifierWithCopy,
  type ExplorerTableColumn,
  type SortDirection,
} from '@rumblefish/soroban-block-explorer-ui';
import { Typography } from '@mui/material';

import { TransactionTime } from '../transactions/TransactionTime.js';

interface LedgersTableProps {
  rows: readonly LedgerListItem[];

  sortDir?: SortDirection;
  /** `(columnId, direction)` — forwarded straight from the sorted column. */
  onSortChange?: (id: string, dir: SortDirection) => void;
  /** Flash newly-appended rows — for the live home feed only; paginated
   *  pages replace every key and would flash the whole page. */
  highlightNewRows?: boolean;
}

function makeColumns(sortable: boolean): ExplorerTableColumn<LedgerListItem>[] {
  return [
    {
      id: 'sequence',
      header: 'Sequence',
      sortable,
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

      cell: (row) => (
        <Chip size="sm" color="neutral" label={String(row.protocol_version)} />
      ),
    },
    {
      id: 'tx_count',
      header: 'TX Count',
      align: 'right',
      cell: (row) => (
        <Typography component="span" variant="bodySmRegular">
          {formatInteger(row.transaction_count)}
        </Typography>
      ),
    },
  ];
}

/** Column count — used to size the loading skeleton consistently. */
export const LEDGER_COLUMN_COUNT = makeColumns(false).length;

/**
 * The Ledgers list table — sequence, hash, closed-at, protocol and
 * transaction-count columns, per the Figma design.
 */
export function LedgersTable({
  rows,
  sortDir,
  onSortChange,
  highlightNewRows = false,
}: LedgersTableProps) {
  const sortable = sortDir != null && onSortChange != null;
  const columns = makeColumns(sortable);
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => String(row.sequence)}
      highlightNewRows={highlightNewRows}
      {...(sortable
        ? {
            sortBy: 'sequence',
            sortDir,
            onSortChange,
          }
        : {})}
    />
  );
}
