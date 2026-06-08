import { Box, Stack, Typography } from '@mui/material';
import type { PoolAssetLeg, PoolItem } from '@rumblefish/api-types';
import {
  Dash,
  ExplorerTable,
  formatAmount,
  IdentifierDisplay,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { routes } from '../../router/routes.js';
// `assetLegLabel` + `legHref` live in the detail-page helpers but the
// labelling + linking rules apply equally to the list — reuse the
// shared helpers rather than duplicating, to keep native-asset / SAC
// mirror / classic-credit precedence in one place.
import {
  assetLegLabel,
  legHref,
  reserveDotColor,
} from '../pool-detail/helpers.js';

import { PoolAssetPair } from './PoolAssetPair.js';
import { FeePill } from './FeePill.js';

export const POOL_COLUMN_COUNT = 5;

/** Render leg code text — wrapped in RouterLink when legHref resolves
 *  (classic credit, SAC mirror); plain text otherwise (native, schema
 *  drift). Matches the precedence used by PoolSummary + PoolKpiStrip. */
function assetCodeNode(leg: PoolAssetLeg): ReactNode {
  const code = assetLegLabel(leg);
  const href = legHref(leg);
  if (!href) return code;
  return (
    <IdentifierDisplay
      value={code}
      type="asset"
      truncate={false}
      href={href}
      fontSize="inherit"
    />
  );
}

/** Colored dot for the per-leg reserves rows — color comes from the
 *  same per-asset `assetColor` hash that drives the leg `AssetIcon`. */
function AssetDot({ color }: { color: string }) {
  return (
    <Box
      component="span"
      sx={{
        display: 'inline-block',
        width: 8,
        height: 8,
        borderRadius: '50%',
        bgcolor: color,
        flexShrink: 0,
      }}
    />
  );
}

const columns: ExplorerTableColumn<PoolItem>[] = [
  {
    id: 'pool',
    header: 'Pool',
    cell: (row) => {
      const pair = `${assetLegLabel(row.asset_a)} / ${assetLegLabel(
        row.asset_b
      )}`;
      return (
        <Stack
          direction="row"
          spacing={1}
          alignItems="center"
          sx={{ minWidth: 0 }}
        >
          <PoolAssetPair a={row.asset_a} b={row.asset_b} />
          <Stack spacing={0.25} sx={{ minWidth: 0 }}>
            <Typography
              variant="bodySmMedium"
              sx={(theme) => ({ color: theme.palette.text.primary })}
            >
              {pair}
            </Typography>
            <IdentifierDisplay
              value={row.pool_id}
              type="pool"
              href={routes.pool(row.pool_id)}
            />
          </Stack>
        </Stack>
      );
    },
  },
  {
    id: 'fee',
    header: 'Fee',
    cell: (row) => <FeePill raw={row.fee_percent} />,
  },
  {
    id: 'reserves',
    header: 'Reserves',
    cell: (row) => {
      // Stale pools (no fresh snapshot) come back with null reserves —
      // render an em-dash rather than "0".
      if (row.reserve_a == null && row.reserve_b == null) return <Dash />;
      return (
        <Stack spacing={0.5}>
          <Stack direction="row" spacing={1} alignItems="center">
            <AssetDot color={reserveDotColor(row.asset_a)} />
            <Typography variant="bodyXsMedium" component="span">
              {row.reserve_a != null ? formatAmount(row.reserve_a) : '—'}{' '}
              {assetCodeNode(row.asset_a)}
            </Typography>
          </Stack>
          <Stack direction="row" spacing={1} alignItems="center">
            <AssetDot color={reserveDotColor(row.asset_b)} />
            <Typography variant="bodyXsMedium" component="span">
              {row.reserve_b != null ? formatAmount(row.reserve_b) : '—'}{' '}
              {assetCodeNode(row.asset_b)}
            </Typography>
          </Stack>
        </Stack>
      );
    },
  },
  {
    id: 'total_shares',
    // Figma reuses the "Reserves" header for this column too. Use a
    // distinct label so screen readers (and column-mapping helpers)
    // don't see two identical headers — visually it still reads as a
    // "reserves" sibling because of the right-aligned amount + "shares"
    // unit label below.
    header: 'Total shares',
    align: 'right',
    cell: (row) => {
      if (row.total_shares == null) return <Dash />;
      return (
        <Stack spacing={0.25} alignItems="flex-end">
          <Typography
            variant="bodySmMedium"
            sx={(theme) => ({ color: theme.palette.text.primary })}
          >
            {formatAmount(row.total_shares)}
          </Typography>
          <Typography
            variant="bodyXsRegular"
            sx={(theme) => ({ color: theme.palette.text.secondary })}
          >
            shares
          </Typography>
        </Stack>
      );
    },
  },
  {
    id: 'participants',
    header: 'Participants',
    align: 'right',
    cell: (row) => (
      <Typography
        variant="bodySmMedium"
        sx={(theme) => ({ color: theme.palette.text.primary })}
      >
        {formatAmount(row.participant_count)}
      </Typography>
    ),
  },
];

interface PoolsTableProps {
  rows: readonly PoolItem[];
}

/**
 * Table for the liquidity-pools list page. Columns mirror the Figma node
 * `266:36052` design: Pool (stacked color-coded asset avatars + pair +
 * truncated id) / Fee (success pill) / Reserves (per-leg) / Total
 * shares (right-aligned, unit label) / Participants.
 */
export function PoolsTable({ rows }: PoolsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => row.pool_id}
    />
  );
}
