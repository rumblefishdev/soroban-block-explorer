import { Box, Stack, Typography } from '@mui/material';
import type { AssetItem } from '@rumblefish/api-types';
import {
  Chip,
  Dash,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  formatAmount,
  IdentifierDisplay,
  IdentifierWithCopy,
  scaleByDecimals,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';

import { AssetIcon } from './AssetIcon.js';
import { assetTypeMeta } from './assetType.js';

const columns: ExplorerTableColumn<AssetItem>[] = [
  {
    id: 'token',
    header: 'Token',
    width: 240,
    cell: (row) => {
      const meta = assetTypeMeta(row.asset_type_name);
      // Soroban-native tokens have no classic asset_code; fall back to the
      // on-chain SEP-41 symbol as the token label (task 0304).
      const label = row.asset_code ?? row.symbol;
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
              {label ? (
                <IdentifierDisplay
                  value={label}
                  type="asset"
                  truncate={false}
                  href={routes.asset(row.id)}
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
    width: 160,
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
    width: 150,
    cell: (row) => {
      // Supply unit: classic asset_code, else the Soroban SEP-41 symbol so
      // the amount reads e.g. "1.5 USDC" instead of bare (task 0304).
      const unit = row.asset_code ?? row.symbol;
      return (
        <Stack sx={{ alignItems: 'flex-end' }}>
          <Typography variant="bodySmRegular">
            {formatAmount(scaleByDecimals(row.total_supply, row.decimals))}
          </Typography>
          {unit && (
            <Typography
              variant="bodyXsRegular"
              sx={(theme) => ({ color: theme.palette.text.tertiary })}
            >
              {unit}
            </Typography>
          )}
        </Stack>
      );
    },
  },
  {
    id: 'holders',
    header: 'Holders',
    align: 'right',
    width: 110,
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
  loading?: boolean;
  skeletonRows?: number;
}

/** The assets list table — token, issuer/contract, supply and holder count. */
export function AssetsTable({ rows, loading, skeletonRows }: AssetsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => row.id}
      rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      loading={loading}
      skeletonRows={skeletonRows}
    />
  );
}
