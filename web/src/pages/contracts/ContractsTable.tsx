import { Stack, Typography } from '@mui/material';
import type { ContractListItem } from '@rumblefish/api-types';
import {
  Chip,
  ExplorerTable,
  IdentifierDisplay,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';
import { formatAmount } from '../format.js';
import { Dash } from '../transactions/cells.js';

import { contractTypeMeta } from './contractType.js';

const columns: ExplorerTableColumn<ContractListItem>[] = [
  {
    id: 'contract',
    header: 'Contract',
    cell: (row) => (
      <IdentifierDisplay
        value={row.contract_id}
        type="contract"
        href={routes.contract(row.contract_id)}
      />
    ),
  },
  {
    id: 'type',
    header: 'Type',
    cell: (row) => {
      const meta = contractTypeMeta(row.contract_type_name);
      return (
        <Stack direction="row" spacing={1} alignItems="center">
          <Chip size="sm" color={meta.color} label={meta.label} />
          {row.is_sac && <Chip size="sm" color="brown" label="SAC" />}
        </Stack>
      );
    },
  },
  {
    id: 'deployed',
    header: 'Deployed at ledger',
    align: 'right',
    cell: (row) =>
      row.deployed_at_ledger != null ? (
        <IdentifierDisplay
          value={String(row.deployed_at_ledger)}
          type="ledger"
        />
      ) : (
        <Dash />
      ),
  },
  {
    id: 'deployer',
    header: 'Deployer',
    cell: (row) =>
      row.deployer ? (
        <IdentifierDisplay
          value={row.deployer}
          type="account"
          href={routes.account(row.deployer)}
        />
      ) : (
        <Dash />
      ),
  },
  {
    id: 'recent_invocations',
    header: 'Invocations (7d)',
    align: 'right',
    cell: (row) => (
      <Typography variant="bodySmRegular">
        {formatAmount(row.recent_invocations)}
      </Typography>
    ),
  },
];

/** Number of columns — sizes the loading skeleton consistently. */
export const CONTRACT_COLUMN_COUNT = columns.length;

interface ContractsTableProps {
  rows: readonly ContractListItem[];
}

/** The contracts list table — contract id, type, deploy ledger,
 *  deployer, and a 7-day invocation count. */
export function ContractsTable({ rows }: ContractsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => row.contract_id}
    />
  );
}
