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
import { assetDisplayCode, assetTypeMeta, SAC_TAG } from './assetType.js';

const columns: ExplorerTableColumn<AssetItem>[] = [
  {
    id: 'token',
    header: 'Token',
    width: 240,
    cell: (row) => {
      const typeMeta = assetTypeMeta(row.asset_type_name);
      // Native XLM and Soroban tokens carry no classic asset_code — the shared
      // rule names them (`XLM` / SEP-41 symbol, tasks 0304 + 0472). The avatar
      // takes the same label, so its letter matches the name beside it.
      const label = assetDisplayCode(row);
      return (
        <Stack
          direction="row"
          spacing={1.5}
          alignItems="center"
          sx={{ minWidth: 0 }}
        >
          <AssetIcon code={label} iconUrl={row.icon_url} />
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
    // Read as one rule: **the issuer when the asset has one, otherwise whatever
    // contract identifies it.** `issuer` is tested FIRST deliberately — it is
    // the only branch whose condition is independent of the contract columns,
    // so no future merge of `contract_id` and `sac_contract_id` can displace it.
    //
    // It has to be first rather than merely present. Until task 0450 the SAC
    // branch preceded the issuer, so every wrapped classic asset (USDC among
    // them) rendered a `C…` address and never its `G…` issuer — the column
    // contradicted its own header and left the domain chip nothing to attach
    // to. Ordering `contract_id` first would work today (`issuer_id` is 0 for
    // native and soroban-native, so the two are mutually exclusive) but puts
    // the trap one refactor away.
    //
    // The SAC address is NOT shown here. That is a real trade — it used to be
    // one glance, now it is one click — but the alternatives are worse: a
    // second address crowds the cell, and linking the Token column's `SAC` chip
    // would make one chip navigate while the identical-looking type badges do
    // not. It lives on the asset detail page instead, untruncated and copyable
    // (`assets/AssetSummary.tsx`). An un-deployed SAC is a reserved address,
    // not a live contract, hence the `linked` gate (ADR 0051, subsumes 0337).
    cell: (row) =>
      row.issuer ? (
        <Stack
          direction="row"
          spacing={1}
          alignItems="center"
          sx={{ minWidth: 0 }}
        >
          <IdentifierWithCopy value={row.issuer} type="account" />
          <DomainChip domain={row.issuer_home_domain} />
        </Stack>
      ) : row.contract_id ? (
        <IdentifierWithCopy value={row.contract_id} type="contract" />
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
      // Supply unit — same display rule as the token cell (0304 + 0472), so
      // the amount reads "1.5 USDC" / "… XLM" instead of bare.
      const unit = assetDisplayCode(row);
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
