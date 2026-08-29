import { Box, Chip, Stack, Typography } from '@mui/material';
import type { PoolItem } from '@rumblefish/api-types';
import {
  Dash,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  formatAmount,
  formatCompactAmount,
  formatCompactUsd,
  IdentifierDisplay,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { routes } from '../../router/routes.js';
// Every leg renders through the shared `poolLegViews` model (task 0374):
// classic pools expand their `asset_a`/`asset_b` pair, soroban pools their
// 2–4 `legs[]`, so labelling / linking / scaling rules live in ONE place.
import {
  isSorobanPool,
  poolLegViews,
  poolPairLabel,
  type PoolLegView,
} from '../pool-shared/helpers.js';

import { PoolAssetPair } from '../pool-shared/PoolAssetPair.js';

export const POOL_COLUMN_COUNT = 6;

/** Render leg label text — wrapped in RouterLink when the view resolves a
 *  target; plain text otherwise (unresolved leg / schema drift). */
function legLabelNode(leg: PoolLegView): ReactNode {
  if (!leg.href) return leg.label;
  return (
    <IdentifierDisplay
      value={leg.label}
      type="asset"
      truncate={false}
      href={leg.href}
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
    width: 260,
    cell: (row) => {
      const legs = poolLegViews(row);
      return (
        <Stack
          direction="row"
          spacing={1}
          alignItems="center"
          sx={{ minWidth: 0 }}
        >
          <PoolAssetPair legs={legs} />
          <Stack spacing={0.25} sx={{ minWidth: 0 }}>
            <Stack direction="row" spacing={0.75} alignItems="center">
              <Typography
                variant="bodySmMedium"
                sx={(theme) => ({ color: theme.palette.text.primary })}
              >
                {poolPairLabel(row)}
              </Typography>
              {/* Protocol chip only when the operator is VERIFIED (task
                  0374 T1) — an unlabelled soroban pool stays unlabelled
                  rather than guessing from shared WASM. */}
              {row.protocol != null && (
                <Chip
                  label={row.protocol}
                  size="small"
                  variant="outlined"
                  sx={{ height: 18, fontSize: 11, textTransform: 'capitalize' }}
                />
              )}
            </Stack>
            <IdentifierDisplay
              value={row.pool_id}
              type={isSorobanPool(row) ? 'contract' : 'pool'}
              href={routes.pool(row.pool_id)}
            />
          </Stack>
        </Stack>
      );
    },
  },
  {
    id: 'reserves',
    header: 'Reserves',
    width: 150,
    cell: (row) => {
      const legs = poolLegViews(row);
      // No reserve on any leg (stale classic pool, or soroban state not
      // yet indexed) — render an em-dash rather than "0".
      if (legs.every((l) => l.reserve == null)) return <Dash />;
      return (
        <Stack spacing={0.5}>
          {legs.map((leg, i) => (
            <Stack
              key={`${leg.label}-${i}`}
              direction="row"
              spacing={1}
              alignItems="center"
            >
              <AssetDot color={leg.dotColor} />
              <Typography variant="bodyXsMedium" component="span">
                {leg.reserve != null ? formatCompactAmount(leg.reserve) : '—'}{' '}
                {legLabelNode(leg)}
              </Typography>
            </Stack>
          ))}
        </Stack>
      );
    },
  },
  {
    id: 'tvl',
    header: 'TVL',
    align: 'right',
    width: 120,
    cell: (row) => {
      // Unpriceable pools (an untracked leg, no fresh snapshot, or a
      // soroban pool whose tokens have no USD series yet) come back with
      // null TVL — em-dash, consistent with the reserves column.
      if (row.tvl == null) return <Dash />;
      return (
        <Typography
          variant="bodySmMedium"
          sx={(theme) => ({ color: theme.palette.text.primary })}
        >
          {formatCompactUsd(row.tvl)}
        </Typography>
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
    width: 150,
    cell: (row) => {
      if (row.total_shares == null) return <Dash />;
      return (
        <Stack spacing={0.25} alignItems="flex-end">
          <Typography
            variant="bodySmMedium"
            sx={(theme) => ({ color: theme.palette.text.primary })}
          >
            {formatCompactAmount(row.total_shares)}
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
    width: 110,
    cell: (row) =>
      // null (soroban list rows — counted per pool on the detail page, not
      // per list row) is NOT zero: em-dash.
      row.participant_count == null ? (
        <Dash />
      ) : (
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
  loading?: boolean;
  skeletonRows?: number;
}

/**
 * Table for the liquidity-pools list page. Columns mirror the Figma node
 * `266:36052` design: Pool (stacked color-coded asset avatars + pair +
 * truncated id) / Reserves (per-leg) / TVL (USD, task 0199 Phase A2 —
 * issue #367's ask; em-dash when a leg is unpriceable) / Total shares
 * (right-aligned, unit label) / Participants. Fee column dropped (task
 * 0348 F9): every classic pool is protocol-fixed at 0.30%
 * (`LIQUIDITY_POOL_FEE_V18`), so a per-row Fee column carried no
 * comparative signal. Since task 0374 the list is a UNION: soroban AMM
 * rows render their 2–4 `legs[]` plus a protocol chip through the same
 * columns.
 */
export function PoolsTable({ rows, loading, skeletonRows }: PoolsTableProps) {
  return (
    <ExplorerTable
      columns={columns}
      rows={rows}
      rowKey={(row) => row.pool_id}
      rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      loading={loading}
      skeletonRows={skeletonRows}
    />
  );
}
