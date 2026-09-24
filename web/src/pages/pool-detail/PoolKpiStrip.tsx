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
  isPoolStale,
  legHref,
  reserveDotColor,
} from '../pool-shared/helpers.js';

const STALE_SUBTITLE = 'no recent snapshot';
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
 * A missing value renders "—", and its caption says which absence it is:
 * "no recent snapshot" for a classic pool whose snapshot went stale, "not
 * indexed" otherwise. A value that IS present is never stale-captioned — a
 * Soroban pool has no snapshots at all, and keying the caption off snapshot
 * freshness told every one of them "no recent snapshot" beside a current
 * reserve. `participant_count` stays accurate regardless (per task 0246).
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
  // "No recent snapshot" is only true of a pool that HAS snapshots; a Soroban
  // pool never does (the table is classic-only).
  const stale =
    pool.latest_snapshot_ledger != null && isPoolStale(pool.latest_snapshot_at);
  const absent = stale ? STALE_SUBTITLE : UNKNOWN_SUBTITLE;

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
        caption={pool.total_shares != null ? 'shares outstanding' : absent}
      />
      {pool.legs.map((leg, i) => {
        const code = assetLegLabel(leg);
        return (
          <KpiCell
            key={i}
            label={`${code} reserve`}
            value={formatCompactAmount(leg.reserve)}
            caption={leg.reserve != null ? assetSubtitle(leg, code) : absent}
            valueColor={reserveDotColor(leg)}
          />
        );
      })}
      <KpiCell
        label="Participants"
        value={formatInteger(pool.participant_count)}
        caption="liquidity providers"
      />
    </Stack>
  );
}
