import { Box, Stack, Typography } from '@mui/material';
import type { AssetItem } from '@rumblefish/api-types';
import {
  Chip,
  Dash,
  DomainChip,
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
import { assetTypeMeta, SAC_TAG } from './assetType.js';

const columns: ExplorerTableColumn<AssetItem>[] = [
  {
    id: 'token',
    header: 'Token',
    width: 240,
    cell: (row) => {
      const typeMeta = assetTypeMeta(row.asset_type_name);
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
              <Chip size="sm" color={typeMeta.color} label={typeMeta.label} />
              {row.sac_deployed && (
                <Chip size="sm" color={SAC_TAG.color} label={SAC_TAG.label} />
              )}
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
    // Wider than a plain identifier: this cell also carries the issuer's
    // home-domain chip. Matches the accounts list, which sized for the same
    // content (task 0450).
    width: 240,
    // Soroban-native tokens have no issuer — the contract IS the asset, so it
    // is all this cell can show, and it is always linked.
    //
    // A classic asset is shown by its ISSUER even when it has a SAC facet. The
    // SAC used to win here, which meant every wrapped classic asset (USDC among
    // them) rendered a `C…` address and never its issuer — the column contradicted
    // its own header, and there was nothing for the domain chip to attach to
    // (task 0450). Nothing is lost by the reorder: the Token column already
    // flags a deployed SAC with its own chip, and the SAC address itself is on
    // the asset detail page. An un-deployed SAC is a reserved address, not a
    // live contract, hence the `linked` gate (ADR 0051, subsumes 0337).
    cell: (row) =>
      row.contract_id ? (
        <IdentifierWithCopy value={row.contract_id} type="contract" />
      ) : row.issuer ? (
        <Stack
          direction="row"
          spacing={1}
          alignItems="center"
          sx={{ minWidth: 0 }}
        >
          <IdentifierWithCopy value={row.issuer} type="account" />
          <DomainChip domain={row.issuer_home_domain} />
        </Stack>
      ) : row.sac_contract_id ? (
        <IdentifierWithCopy
          value={row.sac_contract_id}
          type="contract"
          linked={row.sac_deployed ?? false}
        />
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
