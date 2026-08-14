import { Stack, Typography } from '@mui/material';
import type { AccountListItem } from '@rumblefish/api-types';
import {
  DomainChip,
  ExplorerTable,
  IdentifierDisplay,
  IdentifierWithCopy,
  Dash,
  formatAmount,
  scaleByDecimals,
  type ExplorerTableColumn,
  type SortDirection,
} from '@rumblefish/soroban-block-explorer-ui';

const columns: ExplorerTableColumn<AccountListItem>[] = [
  {
    id: 'account',
    header: 'Account',
    // Wider than a plain identifier: this cell also carries the home-domain
    // chip (e.g. "lobstr.co") next to the address + copy button.
    width: 240,
    cell: (row) => (
      <Stack
        direction="row"
        spacing={1}
        alignItems="center"
        sx={{ minWidth: 0 }}
      >
        <IdentifierWithCopy value={row.account_id} type="account" />
        <DomainChip domain={row.home_domain} />
      </Stack>
    ),
  },
  {
    id: 'xlm',
    header: 'XLM Balance',
    align: 'right',
    width: 165,
    cell: (row) =>
      row.xlm_balance != null ? (
        <Typography component="span" variant="bodySmMedium">
          {/* xlm_balance is RAW stroops (Int128); native is 7 decimals. */}
          {formatAmount(scaleByDecimals(row.xlm_balance, 7))}
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
    width: 180,
    cell: (row) => (
      <IdentifierDisplay value={String(row.last_seen_ledger)} type="ledger" />
    ),
  },
  {
    id: 'first_seen',
    header: 'First Seen',
    align: 'right',
    width: 180,
    cell: (row) => (
      <IdentifierDisplay value={String(row.first_seen_ledger)} type="ledger" />
    ),
  },
];

interface AccountsTableProps {
  rows: readonly AccountListItem[];
  sortDir?: SortDirection;
  /** `(columnId, direction)` — forwarded straight from the sorted column. */
  onSortChange?: (id: string, dir: SortDirection) => void;
  loading?: boolean;
  skeletonRows?: number;
}

export function AccountsTable({
  rows,
  sortDir,
  onSortChange,
  loading,
  skeletonRows,
}: AccountsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => row.account_id}
      sortBy="last_seen"
      sortDir={sortDir}
      onSortChange={onSortChange}
      loading={loading}
      skeletonRows={skeletonRows}
    />
  );
}

export const ACCOUNT_COLUMN_COUNT = 4;
