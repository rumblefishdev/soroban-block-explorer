import { Stack } from '@mui/material';
import type { PoolAssetLeg, PoolItem } from '@rumblefish/api-types';
import {
  formatCompactAmount,
  formatInteger,
  IdentifierDisplay,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { KpiCell } from '../detail/KpiCell.js';
import { assetLegColor } from '../liquidity-pools/assetColor.js';

import { assetLegLabel, isPoolStale, legHref } from './helpers.js';

const STALE_SUBTITLE = 'no recent snapshot';

interface PoolKpiStripProps {
  pool: PoolItem;
}

/**
 * Four-cell KPI strip above the Summary card on the LP detail page —
 * Total shares, per-leg reserves, and participant count. Reserves render
 * with compact notation (`1.2M`, `480K`); the subtitle carries the asset
 * code so the value reads cleanly without units stacked on top.
 *
 * Stale pools (no fresh snapshot in 7 days) come back with null reserves
 * and shares — those cells render as "—". `participant_count` stays
 * accurate regardless of freshness (per task 0246).
 */
function assetSubtitle(leg: PoolAssetLeg, code: string): ReactNode {
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

export function PoolKpiStrip({ pool }: PoolKpiStripProps) {
  const codeA = assetLegLabel(pool.asset_a);
  const codeB = assetLegLabel(pool.asset_b);
  const stale = isPoolStale(pool.latest_snapshot_at);

  return (
    <Stack
      direction={{ xs: 'column', sm: 'row' }}
      spacing={{ xs: 2, sm: 3 }}
      sx={{ width: '100%' }}
    >
      <KpiCell
        label="Total shares"
        value={formatCompactAmount(pool.total_shares)}
        caption={stale ? STALE_SUBTITLE : 'shares outstanding'}
      />
      <KpiCell
        label={`${codeA} reserve`}
        value={formatCompactAmount(pool.reserve_a)}
        caption={stale ? STALE_SUBTITLE : assetSubtitle(pool.asset_a, codeA)}
        valueColor={assetLegColor(pool.asset_a).dot}
      />
      <KpiCell
        label={`${codeB} reserve`}
        value={formatCompactAmount(pool.reserve_b)}
        caption={stale ? STALE_SUBTITLE : assetSubtitle(pool.asset_b, codeB)}
        valueColor={assetLegColor(pool.asset_b).dot}
      />
      <KpiCell
        label="Participants"
        value={formatInteger(pool.participant_count)}
        caption="liquidity providers"
      />
    </Stack>
  );
}
