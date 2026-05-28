import { Box, Link, Stack, Typography } from '@mui/material';
import type { AssetItem } from '@rumblefish/api-types';
import {
  Chip,
  ExplorerTable,
  IdentifierWithCopy,
  type ExplorerTableColumn,
  type SortDirection,
} from '@rumblefish/soroban-block-explorer-ui';
import { Link as RouterLink } from 'react-router-dom';

import { routes } from '../../router/routes.js';
import { formatAmount } from '../format.js';
import { Dash } from '../transactions/cells.js';

import { AssetIcon } from './AssetIcon.js';
import { assetTypeMeta, iconKindFor } from './assetType.js';

const columns: ExplorerTableColumn<AssetItem>[] = [
  {
    id: 'token',
    header: 'Token',
    cell: (row) => {
      const meta = assetTypeMeta(row.asset_type_name);
      return (
        <Stack
          direction="row"
          spacing={1.5}
          alignItems="center"
          sx={{ minWidth: 0 }}
        >
          <AssetIcon
            code={row.asset_code}
            iconUrl={row.icon_url}
            kind={iconKindFor(row.asset_type_name)}
          />
          <Box sx={{ minWidth: 0 }}>
            <Stack direction="row" spacing={1} alignItems="center">
              <Link
                component={RouterLink}
                to={routes.asset(String(row.id))}
                variant="bodySmMedium"
                sx={{ color: 'text.primary' }}
              >
                {row.asset_code ?? '—'}
              </Link>
              <Chip size="sm" color={meta.color} label={meta.label} />
            </Stack>
            {row.name && (
              <Typography
                variant="bodyXsRegular"
                sx={{ color: 'text.tertiary' }}
              >
                {row.name}
              </Typography>
            )}
          </Box>
        </Stack>
      );
    },
  },
  {
    id: 'issuer',
    header: 'Issuer / Contract ID',
    cell: (row) =>
      row.contract_id ? (
        <IdentifierWithCopy value={row.contract_id} type="contract" />
      ) : row.issuer ? (
        <IdentifierWithCopy value={row.issuer} type="account" />
      ) : (
        <Dash />
      ),
  },
  {
    id: 'supply',
    header: 'Total supply',
    align: 'right',
    sortable: true,
    cell: (row) => (
      <Stack sx={{ alignItems: 'flex-end' }}>
        <Typography variant="bodySmRegular">
          {formatAmount(row.total_supply)}
        </Typography>
        {row.asset_code && (
          <Typography variant="bodyXsRegular" sx={{ color: 'text.tertiary' }}>
            {row.asset_code}
          </Typography>
        )}
      </Stack>
    ),
  },
  {
    id: 'holders',
    header: 'Holders',
    align: 'right',
    cell: (row) => (
      <Typography variant="bodySmRegular">
        {formatAmount(row.holder_count)}
      </Typography>
    ),
  },
];

/** Number of columns — used to size the loading skeleton consistently. */
export const ASSET_COLUMN_COUNT = columns.length;

interface AssetsTableProps {
  rows: readonly AssetItem[];
  sortDir: SortDirection;
  onSortChange: (dir: SortDirection) => void;
}

/** The assets list table — token, issuer/contract, supply and holder count. */
export function AssetsTable({ rows, sortDir, onSortChange }: AssetsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => String(row.id)}
      sortBy="supply"
      sortDir={sortDir}
      onSortChange={(_id, dir) => onSortChange(dir)}
    />
  );
}
