import GroupIcon from '@mui/icons-material/GroupOutlined';
import { Typography } from '@mui/material';
import type { ParticipantItem } from '@rumblefish/api-types';
import {
  EmptyState,
  ExplorerTable,
  IdentifierDisplay,
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
    cell: (row) => <IdentifierWithCopy value={row.account} type="account" />,
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
        {/* null = unknown scale (a soroban share token that never published
            decimals) — em-dash; Share % is scale-free and still reports. */}
        {row.shares != null ? formatAmount(row.shares) : '—'}
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
  {
    id: 'first_deposit_ledger',
    header: 'Since ledger',
    align: 'right',
    width: 120,
    cell: (row) =>
      // null for soroban share-token holders — `balances` records current
      // state, not the first sighting.
      row.first_deposit_ledger != null ? (
        <IdentifierDisplay
          value={String(row.first_deposit_ledger)}
          type="ledger"
        />
      ) : (
        '—'
      ),
  },
];

interface PoolParticipantsProps {
  poolId: string;
}

/**
 * The API refuses participants for a soroban pool with no known share
 * token — a DELIBERATE 400 (`INVALID_FILTER`), not a transient failure.
 * Rendering it as "Something went wrong / Try again" lies twice: retry can
 * never succeed, and a permanent refusal reads as an outage (ADR 0058 §5
 * promises an explanatory state instead).
 */
function isNoShareTokenRefusal(error: unknown): boolean {
  if (!(error instanceof Error)) return false;
  const { status, body } = error as Error & {
    status?: number;
    body?: { code?: string };
  };
  return status === 400 && body?.code === 'INVALID_FILTER';
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
  } else if (isError && isNoShareTokenRefusal(error)) {
    body = (
      <EmptyState
        icon={<GroupIcon />}
        title="Participants not indexed for this pool"
        description="No share token is known for this pool — either not yet derived, or a concentrated pool whose positions are not share-token balances."
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
