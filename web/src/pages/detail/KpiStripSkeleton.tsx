import { Stack } from '@mui/material';

import { KpiCell } from './KpiCell.js';

interface KpiStripSkeletonProps {
  /** One entry per cell; label/caption shown, value rendered as a skeleton. */
  cells: { label: string; caption?: string }[];
}

/**
 * Loading placeholder for a horizontal KPI strip (e.g. `PoolKpiStrip`,
 * contract KPI row). Renders N `KpiCell`s in their loading state in the same
 * responsive Stack layout as the real strip, so it doesn't shift on load.
 */
export function KpiStripSkeleton({ cells }: KpiStripSkeletonProps) {
  return (
    <Stack
      direction={{ xs: 'column', sm: 'row' }}
      spacing={{ xs: 2, sm: 3 }}
      sx={{ width: '100%' }}
    >
      {cells.map((c, i) => (
        <KpiCell key={i} label={c.label} caption={c.caption} loading />
      ))}
    </Stack>
  );
}
