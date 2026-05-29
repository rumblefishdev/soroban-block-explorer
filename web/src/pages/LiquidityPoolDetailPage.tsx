import { Stack } from '@mui/material';
import {
  CardSkeleton,
  classifyError,
  GenericErrorState,
  isMissingResource,
  isPoolId,
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
  // The `= ''` fallback is only here to narrow `id` from
  // `string | undefined` (the `useParams` return type) down to `string`
  // for the hooks below. React Router guarantees `:id` is present and
  // non-empty whenever this component mounts, so the fallback value is
  // never actually observed at runtime.
  const { id = '' } = useParams<{ id: string }>();
  const poolId = id;
  // Pool ids must be a CAP-38 `L...` strkey (56 chars, base32). Validate
  // up-front so a malformed id renders the entity-specific NotFoundState
  // instead of firing a doomed request. `usePoolDetail` is hardcoded to
  // skip the network when the id is empty.
  const validPoolId = isPoolId(poolId);
  const detail = usePoolDetail(validPoolId ? poolId : '');
  if (!validPoolId) {
    return <NotFoundState entity="liquidity-pool" identifier={poolId} />;
  }

  let summarySection: ReactNode = null;
  let kpiSection: ReactNode = null;
  if (detail.isLoading) {
    kpiSection = <CardSkeleton />;
    summarySection = <CardSkeleton />;
  } else if (detail.isError) {
    const kind = classifyError(detail.error);
    summarySection = isMissingResource(kind) ? (
      <NotFoundState entity="liquidity-pool" identifier={poolId} py={6} />
    ) : (
      <GenericErrorState onRetry={() => void detail.refetch()} />
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
      {!detail.isError && (
        <>
          <SectionErrorBoundary sectionName="pool-charts">
            <PoolCharts poolId={poolId} />
          </SectionErrorBoundary>
          <SectionErrorBoundary sectionName="pool-participants">
            <PoolParticipants poolId={poolId} />
          </SectionErrorBoundary>
          <SectionErrorBoundary sectionName="pool-transactions">
            <PoolTransactions poolId={poolId} />
          </SectionErrorBoundary>
        </>
      )}
    </Stack>
  );
}
