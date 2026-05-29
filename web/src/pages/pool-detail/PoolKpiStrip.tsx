import { Card, Stack, Typography } from '@mui/material';
import type { PoolAssetLeg, PoolItem } from '@rumblefish/api-types';
import { IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { assetLegColor } from '../liquidity-pools/assetColor.js';

import {
  assetLegLabel,
  formatCompactAmount,
  isPoolStale,
  legHref,
} from './helpers.js';

const STALE_SUBTITLE = 'no recent snapshot';

/** Module-level formatter — Intl.NumberFormat is expensive to
 *  instantiate on every render. */
const COUNT_FORMATTER = new Intl.NumberFormat('en-US');

interface KpiCellProps {
  label: string;
  value: ReactNode;
  subtitle: ReactNode;
  /**
   * Optional override for the headline value color. Used by the per-leg
   * reserve cells so the number reads in the asset's brand hue (Figma
   * node `325:22339` — XLM blue, USDC emerald, etc.). Defaults to the
   * primary text color when omitted.
   */
  valueColor?: string;
}

function KpiCell({ label, value, subtitle, valueColor }: KpiCellProps) {
  return (
    <Card sx={{ p: 2, flex: 1, minWidth: 0 }}>
      <Stack spacing={1}>
        <Typography variant="bodyXsRegular" sx={{ color: 'text.tertiary' }}>
          {label}
        </Typography>
        <Typography
          variant="heading4SemiBold"
          component="div"
          sx={{ color: valueColor ?? 'text.primary' }}
        >
          {value}
        </Typography>
        <Typography variant="bodyXsRegular" sx={{ color: 'text.tertiary' }}>
          {subtitle}
        </Typography>
      </Stack>
    </Card>
  );
}

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
      spacing={2}
      sx={{ width: '100%' }}
    >
      <KpiCell
        label="Total shares"
        value={formatCompactAmount(pool.total_shares)}
        subtitle={stale ? STALE_SUBTITLE : 'shares outstanding'}
      />
      <KpiCell
        label={`${codeA} reserve`}
        value={formatCompactAmount(pool.reserve_a)}
        subtitle={stale ? STALE_SUBTITLE : assetSubtitle(pool.asset_a, codeA)}
        valueColor={assetLegColor(pool.asset_a).dot}
      />
      <KpiCell
        label={`${codeB} reserve`}
        value={formatCompactAmount(pool.reserve_b)}
        subtitle={stale ? STALE_SUBTITLE : assetSubtitle(pool.asset_b, codeB)}
        valueColor={assetLegColor(pool.asset_b).dot}
      />
      <KpiCell
        label="Participants"
        value={COUNT_FORMATTER.format(pool.participant_count)}
        subtitle="liquidity providers"
      />
    </Stack>
  );
}
