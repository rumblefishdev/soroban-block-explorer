import { Stack, Typography } from '@mui/material';
import type { ContractListItem } from '@rumblefish/api-types';
import {
  Chip,
  ExplorerTable,
  IdentifierDisplay,
  IdentifierWithCopy,
  Dash,
  formatAmount,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';

import { contractTypeMeta } from './contractType.js';

const columns: ExplorerTableColumn<ContractListItem>[] = [
  {
    id: 'contract',
    header: 'Contract',
    width: 160,
    cell: (row) => (
      <IdentifierWithCopy
        value={row.contract_id}
        type="contract"
        href={routes.contract(row.contract_id)}
      />
    ),
  },
  {
    id: 'type',
    header: 'Type',
    width: 120,
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
    width: 120,
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
    width: 160,
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
    // Overridden per-render from the row's `stats_window` — see below.
    header: 'Invocations',
    align: 'right',
    width: 110,
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
  loading?: boolean;
  skeletonRows?: number;
}

/** The contracts list table — contract id, type, deploy ledger,
 *  deployer, and a windowed invocation count. */
export function ContractsTable({
  rows,
  loading,
  skeletonRows,
}: ContractsTableProps) {
  // The window comes off the row rather than being hardcoded: `stats_window`
  // is derived from the same constant that bounds the count server-side, so
  // header and number cannot drift if the window ever changes. Hardcoding
  // "(7d)" here was the exact drift the field was added to prevent (0377 F6).
  // Falls back to the bare label while the skeleton has no rows.
  const windowLabel = rows[0]?.stats_window;
  const invocationsHeader =
    windowLabel != null ? `Invocations (${windowLabel})` : 'Invocations';

  return (
    <ExplorerTable
      columns={columns.map((c) =>
        c.id === 'recent_invocations' ? { ...c, header: invocationsHeader } : c
      )}
      rows={rows}
      rowKey={(row) => row.contract_id}
      loading={loading}
      skeletonRows={skeletonRows}
    />
  );
}
