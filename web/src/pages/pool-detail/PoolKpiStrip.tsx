import { Stack } from '@mui/material';
import type { PoolItem } from '@rumblefish/api-types';
import {
  formatCompactAmount,
  formatInteger,
  IdentifierDisplay,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { KpiCell } from '../detail/KpiCell.js';

import {
  isPoolStale,
  poolLegViews,
  type PoolLegView,
} from '../pool-shared/helpers.js';

const STALE_SUBTITLE = 'no recent snapshot';

interface PoolKpiStripProps {
  pool: PoolItem;
}

/**
 * KPI strip above the Summary card on the LP detail page — Total shares,
 * per-leg reserves (2 for classic pairs, up to 4 for soroban stable
 * pools), and participant count. Reserves render with compact notation
 * (`1.2M`, `480K`); the subtitle carries the asset label so the value
 * reads cleanly without units stacked on top.
 *
 * Stale pools (no fresh snapshot in 7 days) come back with null reserves
 * and shares — those cells render as "—". `participant_count` stays
 * accurate regardless of freshness (per task 0246); for soroban pools it
 * is null (counted by the participants section, not per row) and renders
 * as "—", never as 0.
 */
function legSubtitle(leg: PoolLegView): ReactNode {
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

export function PoolKpiStrip({ pool }: PoolKpiStripProps) {
  const legs = poolLegViews(pool);
  const stale = isPoolStale(pool);

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
      {legs.map((leg, i) => (
        <KpiCell
          key={`${leg.label}-${i}`}
          label={`${leg.label} reserve`}
          value={leg.reserve != null ? formatCompactAmount(leg.reserve) : '—'}
          // Staleness must not swallow the asset link (review #438 UX-F1) —
          // the caption keeps the navigation and gains the warning.
          caption={
            stale ? (
              <>
                {legSubtitle(leg)} — {STALE_SUBTITLE}
              </>
            ) : (
              legSubtitle(leg)
            )
          }
          valueColor={leg.dotColor}
        />
      ))}
      <KpiCell
        label="Participants"
        value={
          pool.participant_count != null
            ? formatInteger(pool.participant_count)
            : '—'
        }
        caption="liquidity providers"
      />
    </Stack>
  );
}
