import { Stack, Typography } from '@mui/material';
import type { AccountListItem } from '@rumblefish/api-types';
import {
  Chip,
  ExplorerTable,
  IdentifierDisplay,
  IdentifierWithCopy,
  type ExplorerTableColumn,
  type SortDirection,
} from '@rumblefish/soroban-block-explorer-ui';

import { formatAmount } from '../format.js';
import { Dash } from '../transactions/cells.js';

const columns: ExplorerTableColumn<AccountListItem>[] = [
  {
    id: 'account',
    header: 'Account',
    cell: (row) => (
      <Stack
        direction="row"
        spacing={1}
        alignItems="center"
        sx={{ minWidth: 0 }}
      >
        <IdentifierWithCopy value={row.account_id} type="account" />
        {row.home_domain && (
          <Chip size="sm" color="neutral" label={row.home_domain} />
        )}
      </Stack>
    ),
  },
  {
    id: 'xlm',
    header: 'XLM Balance',
    align: 'right',
    cell: (row) =>
      row.xlm_balance != null ? (
        <Typography component="span" variant="bodySmMedium">
          {formatAmount(row.xlm_balance)}
        </Typography>
      ) : (
        <Dash />
      ),
  },
  {
    id: 'last_seen',
    header: 'Last Seen',
    align: 'right',
    sortable: true,
    cell: (row) => (
      <IdentifierDisplay value={String(row.last_seen_ledger)} type="ledger" />
    ),
  },
  {
    id: 'first_seen',
    header: 'First Seen',
    align: 'right',
    cell: (row) => (
      <IdentifierDisplay value={String(row.first_seen_ledger)} type="ledger" />
    ),
  },
];

interface AccountsTableProps {
  rows: readonly AccountListItem[];
  sortDir: SortDirection;
  /** `(columnId, direction)` — forwarded straight from the sorted column. */
  onSortChange: (id: string, dir: SortDirection) => void;
}

export function AccountsTable({
  rows,
  sortDir,
  onSortChange,
}: AccountsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => row.account_id}
      sortBy="last_seen"
      sortDir={sortDir}
      onSortChange={onSortChange}
    />
  );
}

export const ACCOUNT_COLUMN_COUNT = 4;
