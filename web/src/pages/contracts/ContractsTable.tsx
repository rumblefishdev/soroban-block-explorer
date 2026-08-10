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

import { Link as RouterLink } from 'react-router-dom';

import { routes } from '../../router/routes.js';

import { contractTypeMeta } from './contractType.js';
import { sacAssetCode, sacAssetId } from './sacAsset.js';

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
          {/* Task 0441: a SAC names the asset it mirrors and links to it;
              the rare unresolvable facet degrades to the bare badge. */}
          {row.is_sac &&
            (row.sac_asset ? (
              <RouterLink
                to={routes.asset(sacAssetId(row.sac_asset))}
                style={{ textDecoration: 'none' }}
              >
                <Chip
                  size="sm"
                  color="brown"
                  clickable
                  label={`SAC · ${sacAssetCode(row.sac_asset)}`}
                />
              </RouterLink>
            ) : (
              <Chip size="sm" color="brown" label="SAC" />
            ))}
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
    header: 'Invocations (7d)',
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
 *  deployer, and a 7-day invocation count. */
export function ContractsTable({
  rows,
  loading,
  skeletonRows,
}: ContractsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => row.contract_id}
      loading={loading}
      skeletonRows={skeletonRows}
    />
  );
}
