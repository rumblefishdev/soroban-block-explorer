import { Stack } from '@mui/material';
import type { PoolAssetLeg, PoolItem } from '@rumblefish/api-types';
import {
  formatCompactAmount,
  formatInteger,
  IdentifierDisplay,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { KpiCell } from '../detail/KpiCell.js';

import {
  assetLegLabel,
  legHref,
  reserveDotColor,
} from '../pool-shared/helpers.js';

const UNKNOWN_SUBTITLE = 'not indexed';

interface PoolKpiStripProps {
  pool: PoolItem;
}

/**
 * KPI strip above the Summary card on the LP detail page — total shares, one
 * cell per leg reserve, and participant count. Reserves render with compact
 * notation (`1.2M`, `480K`); the subtitle carries the asset code so the value
 * reads cleanly without units stacked on top.
 *
 * A missing value renders "—" captioned "not indexed". There is no staleness
 * caption: the API returns a pool's latest state whatever its age, and a
 * classic pool writes a snapshot on every change, so an old snapshot is a
 * quiet pool's CURRENT state, not an outdated one.
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
  return (
    <Stack
      direction={{ xs: 'column', sm: 'row' }}
      // The strip was exactly four cells; it is now two plus one per leg, so a
      // four-leg pool puts six across. Wrapping keeps every reserve visible
      // rather than compressing the labels past reading — `rowGap` because
      // `spacing` only sets the gap along the main axis.
      flexWrap="wrap"
      spacing={{ xs: 2, sm: 3 }}
      sx={{ width: '100%', rowGap: { xs: 2, sm: 3 } }}
    >
      <KpiCell
        label="Total shares"
        value={formatCompactAmount(pool.total_shares)}
        caption={
          pool.total_shares != null ? 'shares outstanding' : UNKNOWN_SUBTITLE
        }
      />
      {pool.legs.map((leg, i) => {
        const code = assetLegLabel(leg);
        return (
          <KpiCell
            key={i}
            label={`${code} reserve`}
            value={formatCompactAmount(leg.reserve)}
            caption={
              leg.reserve != null ? assetSubtitle(leg, code) : UNKNOWN_SUBTITLE
            }
            valueColor={reserveDotColor(leg)}
          />
        );
      })}
      <KpiCell
        label="Participants"
        value={
          pool.participant_count != null
            ? formatInteger(pool.participant_count)
            : '—'
        }
        caption={
          pool.participant_count != null
            ? 'liquidity providers'
            : UNKNOWN_SUBTITLE
        }
      />
    </Stack>
  );
}
