import { Box, Stack, Typography } from '@mui/material';
import type { AssetItem } from '@rumblefish/api-types';
import {
  Chip,
  Dash,
  ExplorerTable,
  formatAmount,
  IdentifierDisplay,
  IdentifierWithCopy,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';

import { AssetIcon } from './AssetIcon.js';
import { assetTypeMeta } from './assetType.js';

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
          <AssetIcon code={row.asset_code} iconUrl={row.icon_url} />
          <Box sx={{ minWidth: 0 }}>
            <Stack direction="row" spacing={1} alignItems="center">
              {row.asset_code ? (
                <IdentifierDisplay
                  value={row.asset_code}
                  type="asset"
                  truncate={false}
                  href={routes.asset(String(row.id))}
                />
              ) : (
                <Dash />
              )}
              <Chip size="sm" color={meta.color} label={meta.label} />
            </Stack>
            {row.name && (
              <Typography
                variant="bodyXsRegular"
                sx={(theme) => ({ color: theme.palette.text.secondary })}
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
    cell: (row) => (
      <Stack sx={{ alignItems: 'flex-end' }}>
        <Typography variant="bodySmRegular">
          {formatAmount(row.total_supply)}
        </Typography>
        {row.asset_code && (
          <Typography
            variant="bodyXsRegular"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
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
}

/** The assets list table — token, issuer/contract, supply and holder count. */
export function AssetsTable({ rows }: AssetsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => String(row.id)}
    />
  );
}
