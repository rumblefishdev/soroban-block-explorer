import { Stack } from '@mui/material';
import { useParams } from 'react-router-dom';

import { KpiStripSkeleton } from '../detail/KpiStripSkeleton.js';
import { SummarySkeleton } from '../detail/SummarySkeleton.js';
import { TableSectionSkeleton } from '../detail/TableSectionSkeleton.js';

import { ChartCardSkeleton } from './ChartCardSkeleton.js';
import { PoolDetailHeader } from './PoolDetailHeader.js';

/** KPI strip cells, mirroring `PoolKpiStrip` (asset codes unknown pre-data, so
 *  the two reserve cells use a generic 'Reserve' label). */
const KPI_CELLS = [
  { label: 'Total shares', caption: 'shares outstanding' },
  { label: 'Reserve' },
  { label: 'Reserve' },
  { label: 'Participants', caption: 'liquidity providers' },
];

/**
 * Faithful loading skeleton for the liquidity-pool detail page — mirrors the
 * full loaded layout (header → KPI strip → summary → charts → participants →
 * transactions) so neither the lazy-chunk fallback (phase A) nor the page's
 * own data-loading state (phase B) jumps as sections resolve. Charts and the
 * two tables are included (not gated) so they reserve their space.
 *
 * Used as BOTH the route Suspense fallback and the page's `isLoading` return.
 * Reuses the real `PoolDetailHeader` (id prop, no fetch) for a pixel-exact
 * header.
 */
export function PoolDetailSkeleton() {
  const { id = '' } = useParams<{ id: string }>();
  return (
    <Stack spacing={3}>
      <PoolDetailHeader poolId={id} pool={undefined} />
      <KpiStripSkeleton cells={KPI_CELLS} />
      <SummarySkeleton title="Summary" rows={4} />
      <ChartCardSkeleton />
      <TableSectionSkeleton title="Pool participants" rows={6} columns={4} />
      <TableSectionSkeleton title="Pool transactions" rows={6} columns={5} />
    </Stack>
  );
}
