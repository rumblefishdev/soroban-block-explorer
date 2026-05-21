import { Box, Stack, Typography } from '@mui/material';
import type { PoolAssetLeg, PoolItem } from '@rumblefish/api-types';
import {
  Chip,
  ExplorerTable,
  IdentifierWithCopy,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { formatAmount } from '../format.js';
import { Dash } from '../transactions/cells.js';

export const POOL_COLUMN_COUNT = 4;

/**
 * Returns the display label for one leg of a pool's asset pair.
 *
 * Native (XLM) legs come back with `asset_type_name === 'native'` and
 * `null` `asset_code`. Classic, SAC, and Soroban legs all carry a
 * code. Falls back to `?` only on schema drift.
 */
function assetLegLabel(leg: PoolAssetLeg): string {
  if (leg.asset_type_name === 'native') return 'XLM';
  return leg.asset_code ?? '?';
}

/** Colored dot per asset leg, matching the Figma reserve row markers. */
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
        <Stack spacing={0.5} sx={{ minWidth: 0 }}>
          <Typography variant="bodySmMedium" sx={{ color: 'text.primary' }}>
            {pair}
          </Typography>
          <IdentifierWithCopy value={row.pool_id} type="pool" />
        </Stack>
      );
    },
  },
  {
    id: 'fee',
    header: 'Fee',
    cell: (row) => (
      <Chip size="sm" color="accent" label={`${row.fee_percent}%`} />
    ),
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
            <AssetDot color="primary.main" />
            <Typography variant="bodyXsMedium">
              {row.reserve_a != null ? formatAmount(row.reserve_a) : '—'}{' '}
              {assetLegLabel(row.asset_a)}
            </Typography>
          </Stack>
          <Stack direction="row" spacing={1} alignItems="center">
            <AssetDot color="success.main" />
            <Typography variant="bodyXsMedium">
              {row.reserve_b != null ? formatAmount(row.reserve_b) : '—'}{' '}
              {assetLegLabel(row.asset_b)}
            </Typography>
          </Stack>
        </Stack>
      );
    },
  },
  {
    id: 'participants',
    header: 'Participants',
    align: 'right',
    cell: (row) => (
      <Typography variant="bodySmRegular">
        {formatAmount(row.participant_count)}
      </Typography>
    ),
  },
];

interface PoolsTableProps {
  rows: PoolItem[];
}

/**
 * Table for the liquidity-pools list page. Columns mirror the Figma node
 * `266:35969` design: Pool (pair + truncated id) / Fee / Reserves
 * (two-line with colored dots) / Participants.
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
