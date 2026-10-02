import GroupIcon from '@mui/icons-material/GroupOutlined';
import HourglassIcon from '@mui/icons-material/HourglassEmptyOutlined';
import { Typography } from '@mui/material';
import type { ParticipantItem } from '@rumblefish/api-types';
import {
  addressType,
  EmptyState,
  ExplorerTable,
  IdentifierWithCopy,
  PaginationControls,
  QueryErrorState,
  useCursorPagination,
  formatAmount,
  formatPercent,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { usePagedRows, usePoolParticipants } from '../../api/index.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { SectionCard } from '../detail/SectionCard.js';

const columns: ExplorerTableColumn<ParticipantItem>[] = [
  {
    id: 'account',
    header: 'Account',
    width: 160,
    // A soroban pool's share token is also held by contracts (gauges, vaults).
    cell: (row) => {
      const type = addressType(row.account);
      return (
        <IdentifierWithCopy
          value={row.account}
          type={type ?? 'account'}
          linked={type != null}
        />
      );
    },
  },
  {
    id: 'shares',
    header: 'Shares',
    align: 'right',
    width: 110,
    cell: (row) => (
      <Typography
        component="span"
        variant="bodySmMedium"
        sx={(theme) => ({ color: theme.palette.text.primary })}
      >
        {formatAmount(row.shares)}
      </Typography>
    ),
  },
  {
    id: 'share_percentage',
    header: 'Share %',
    align: 'right',
    width: 110,
    cell: (row) => (
      <Typography
        component="span"
        variant="bodySmMedium"
        sx={(theme) => ({ color: theme.palette.text.primary })}
      >
        {row.share_percentage != null
          ? formatPercent(Number(row.share_percentage))
          : '—'}
      </Typography>
    ),
  },
];

interface PoolParticipantsProps {
  poolId: string;
}

/**
 * "Pool participants" section of the LP detail page — a paginated list
 * of liquidity providers ordered by shares DESC. Fetched independently
 * of the rest of the page so failures stay scoped.
 */
export function PoolParticipants({ poolId }: PoolParticipantsProps) {
  // Namespaced cursor: LP detail mounts PoolParticipants + PoolActivity
  // simultaneously, so each section needs its own URL key. `resetKey`
  // drops the cursor when the user navigates to a different pool.
  const { cursor, goNext, goPrev } = useCursorPagination({
    cursorParam: CURSOR_PARAMS.POOL_PARTICIPANTS,
    resetKey: poolId,
  });

  const { data, isLoading, isPlaceholderData, isError, error, refetch } =
    usePoolParticipants(poolId, cursor);

  const { rows, canPrev, canNext, handlePrev, handleNext } = usePagedRows(
    data,
    goNext,
    goPrev
  );

  // A concentrated soroban pool keeps positions, not a share token.
  if (
    (error as { body?: { code?: unknown } } | null)?.body?.code ===
    'not_indexed'
  ) {
    return (
      <SectionCard title="Pool participants">
        <EmptyState
          icon={<HourglassIcon />}
          title="Not indexed yet"
          description="Liquidity providers of this pool are not indexed yet."
        />
      </SectionCard>
    );
  }

  let body: ReactNode;
  if (isLoading || isPlaceholderData) {
    body = (
      <ExplorerTable
        columns={columns}
        rows={[]}
        rowKey={(row) => row.account}
        loading
        skeletonRows={20}
      />
    );
  } else if (isError) {
    body = <QueryErrorState error={error} onRetry={() => void refetch()} />;
  } else if (rows.length === 0) {
    body = (
      <EmptyState
        icon={<GroupIcon />}
        title="No participants yet"
        description="This pool currently has no active liquidity providers."
      />
    );
  } else {
    body = (
      <ExplorerTable
        columns={columns}
        rows={rows}
        rowKey={(row) => row.account}
      />
    );
  }

  return (
    <SectionCard title="Pool participants">
      {body}
      <PaginationControls
        caption="Latest results"
        canPrev={canPrev}
        canNext={canNext}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </SectionCard>
  );
}
