import { Link, Stack } from '@mui/material';
import type { PoolAssetLeg, PoolItem } from '@rumblefish/api-types';
import type { ReactNode } from 'react';
import { Link as RouterLink } from 'react-router-dom';

import { KpiCell } from '../detail/KpiCell.js';

import {
  assetLegLabel,
  formatCompactAmount,
  isPoolStale,
  legHref,
  reserveDotColor,
} from './helpers.js';

const STALE_SUBTITLE = 'no recent snapshot';

/** Module-level formatter — Intl.NumberFormat is expensive to
 *  instantiate on every render. */
const COUNT_FORMATTER = new Intl.NumberFormat('en-US');

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
    <Link
      component={RouterLink}
      to={href}
      sx={{
        color: 'inherit',
        textDecoration: 'none',
        '&:hover': { textDecoration: 'underline' },
      }}
    >
      {code}
    </Link>
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
        valueColor={reserveDotColor(pool.asset_a)}
      />
      <KpiCell
        label={`${codeB} reserve`}
        value={formatCompactAmount(pool.reserve_b)}
        caption={stale ? STALE_SUBTITLE : assetSubtitle(pool.asset_b, codeB)}
        valueColor={reserveDotColor(pool.asset_b)}
      />
      <KpiCell
        label="Participants"
        value={COUNT_FORMATTER.format(pool.participant_count)}
        caption="liquidity providers"
      />
    </Stack>
  );
}
