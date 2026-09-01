import { Stack } from '@mui/material';
import type { PoolItem } from '@rumblefish/api-types';
import {
  formatCompactAmount,
  formatInteger,
  IdentifierDisplay,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { KpiCell } from '../detail/KpiCell.js';

import { poolLegViews, type PoolLegView } from '../pool-shared/helpers.js';

interface PoolKpiStripProps {
  pool: PoolItem;
}

/**
 * KPI strip above the Summary card on the LP detail page — Total shares,
 * per-leg reserves (2 for classic pairs, up to 4 for soroban stable
 * pools), and participant count. Reserves render with compact notation
 * (`1.2M`, `480K`); the subtitle carries the asset label so the value reads
 * cleanly without units stacked on top.
 *
 * **No staleness caption, deliberately.** An earlier strip captioned the
 * reserves "no recent snapshot" whenever the newest snapshot was over a week
 * old, which reads as "these numbers may be out of date". It is the opposite:
 * a classic pool writes a snapshot on every change, so an old snapshot means
 * the pool has been IDLE and the reserves beside it are exactly current. It
 * was also the majority state — 60% of classic pools last changed over a week
 * ago, the median 35 days — so the warning fired on most of the list while
 * saying nothing true. Removed rather than reworded: the judgement was never
 * ours to make, and the age is not a fact this endpoint carries.
 *
 * `participant_count` is computed from the live positions table (task 0246);
 * for soroban pools it is null (counted by the participants section, not per
 * row) and renders as "—", never as 0.
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

  return (
    <Stack
      direction={{ xs: 'column', sm: 'row' }}
      spacing={{ xs: 2, sm: 3 }}
      sx={{ width: '100%' }}
    >
      <KpiCell
        label="Total shares"
        value={formatCompactAmount(pool.total_shares)}
        caption="shares outstanding"
      />
      {legs.map((leg, i) => (
        <KpiCell
          key={`${leg.label}-${i}`}
          label={`${leg.label} reserve`}
          value={leg.reserve != null ? formatCompactAmount(leg.reserve) : '—'}
          caption={legSubtitle(leg)}
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
