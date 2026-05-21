import { Box, Stack } from '@mui/material';
import {
  CardSkeleton,
  classifyError,
  GenericErrorState,
  NotFoundState,
  SectionErrorBoundary,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { useParams } from 'react-router-dom';

import { usePoolDetail } from '../api/index.js';

import { PoolCharts } from './pool-detail/PoolCharts.js';
import { PoolDetailHeader } from './pool-detail/PoolDetailHeader.js';
import { PoolKpiStrip } from './pool-detail/PoolKpiStrip.js';
import { PoolParticipants } from './pool-detail/PoolParticipants.js';
import { PoolSummary } from './pool-detail/PoolSummary.js';
import { PoolTransactions } from './pool-detail/PoolTransactions.js';

/**
 * Liquidity-pool detail page (`/liquidity-pools/:id`). Composes the
 * Figma-defined sections from top to bottom:
 *
 *   1. Header (breadcrumb, pair name, Active/Stale badge, truncated id)
 *   2. KPI strip (Total shares, A-leg reserve, B-leg reserve, participants)
 *   3. Summary (key-value rows for Pool ID, Fee, Total shares, reserves)
 *   4. Activity chart (TVL/Volume/Fees tabs, 1D/7D/30D/1Y range)
 *   5. Pool participants table
 *   6. Recent transactions table
 *
 * Each section is fetched by its own query and wrapped in a
 * `SectionErrorBoundary` so one failing section never collapses
 * unrelated siblings.
 */
export default function LiquidityPoolDetailPage() {
  // React Router won't match `/liquidity-pools/:id` without `:id`, so
  // `id` is always non-empty here — no need for a defensive fallback
  // branch that would never execute.
  const { id = '' } = useParams<{ id: string }>();
  const poolId = id;
  const detail = usePoolDetail(poolId);

  let summarySection: ReactNode = null;
  let kpiSection: ReactNode = null;
  if (detail.isLoading) {
    kpiSection = <CardSkeleton />;
    summarySection = <CardSkeleton />;
  } else if (detail.isError) {
    const kind = classifyError(detail.error);
    summarySection = (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 6 }}>
        {kind === 'not-found' ? (
          <NotFoundState entity="liquidity-pool" identifier={poolId} />
        ) : (
          <GenericErrorState onRetry={() => void detail.refetch()} />
        )}
      </Box>
    );
  } else if (detail.data) {
    kpiSection = <PoolKpiStrip pool={detail.data} />;
    summarySection = <PoolSummary pool={detail.data} />;
  }

  return (
    <Stack spacing={3}>
      <PoolDetailHeader poolId={poolId} pool={detail.data} />

      {kpiSection != null && (
        <SectionErrorBoundary sectionName="pool-kpi-strip">
          {kpiSection}
        </SectionErrorBoundary>
      )}
      <SectionErrorBoundary sectionName="pool-summary">
        {summarySection}
      </SectionErrorBoundary>
      <SectionErrorBoundary sectionName="pool-charts">
        <PoolCharts poolId={poolId} />
      </SectionErrorBoundary>
      <SectionErrorBoundary sectionName="pool-participants">
        <PoolParticipants poolId={poolId} />
      </SectionErrorBoundary>
      <SectionErrorBoundary sectionName="pool-transactions">
        <PoolTransactions poolId={poolId} />
      </SectionErrorBoundary>
    </Stack>
  );
}
