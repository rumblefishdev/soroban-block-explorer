import { Typography } from '@mui/material';
import {
  ExplorerTable,
  IdentifierWithCopy,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import type { AccountListItem } from '../../api/hooks/useAccountsList.js';
import { formatAmount } from '../format.js';
import { Dash } from '../transactions/cells.js';

interface AccountsTableProps {
  rows: readonly AccountListItem[];
  startRank?: number;
}

export function AccountsTable({ rows, startRank = 0 }: AccountsTableProps) {
  const columns: ExplorerTableColumn<AccountListItem>[] = [
    {
      id: 'rank',
      header: '#',
      align: 'right',
      width: 56,
      cell: (_row, idx) => (
        <Typography
          component="span"
          variant="bodyMonoSmMedium"
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
        >
          {startRank + idx + 1}
        </Typography>
      ),
    },
    {
      id: 'account',
      header: 'Account',
      cell: (row) => (
        <IdentifierWithCopy value={row.account_id} type="account" />
      ),
    },
    {
      id: 'xlm',
      header: 'XLM Balance',
      align: 'right',
      cell: (row) => (
        <Typography component="span" variant="bodySmMedium">
          {formatAmount(row.xlm_balance)}
        </Typography>
      ),
    },
    {
      id: 'supply',
      header: '% Supply',
      align: 'right',
      cell: (row) => (
        <Typography
          component="span"
          variant="bodyMonoSmMedium"
          sx={(theme) => ({ color: theme.palette.text.secondary })}
        >
          {row.xlm_supply_percent.toFixed(2)}%
        </Typography>
      ),
    },
    {
      id: 'last_seen',
      header: 'Last Seen',
      align: 'right',
      cell: (row) => (
        <Typography component="span" variant="bodyMonoSmMedium">
          {formatAmount(row.last_seen_ledger)}
        </Typography>
      ),
    },
    {
      id: 'first_seen',
      header: 'First Seen',
      align: 'right',
      cell: (row) => (
        <Typography
          component="span"
          variant="bodyMonoSmMedium"
          sx={(theme) => ({ color: theme.palette.text.secondary })}
        >
          {formatAmount(row.first_seen_ledger)}
        </Typography>
      ),
    },
    {
      id: 'domain',
      header: 'Domain',
      cell: (row) =>
        row.home_domain ? (
          <Typography component="span" variant="bodySmMedium">
            {row.home_domain}
          </Typography>
        ) : (
          <Dash />
        ),
    },
  ];
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => row.account_id}
    />
  );
}

export const ACCOUNT_COLUMN_COUNT = 7;
