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
  formatInteger,
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
  {
    id: 'first_deposit_ledger',
    header: 'Since ledger',
    align: 'right',
    width: 120,
    cell: (row) => (
      <IdentifierDisplay
        value={String(row.first_deposit_ledger)}
        type="ledger"
      />
    ),
  },
];

interface PoolParticipantsProps {
  poolId: string;
  /** The pool header's own `participant_count` — decides whether an empty page
   *  means "none" or "we could not list them" (0377 F7). */
  participantCount: number;
}

/**
 * "Pool participants" section of the LP detail page — a paginated list
 * of liquidity providers ordered by shares DESC. Fetched independently
 * of the rest of the page so failures stay scoped.
 */
export function PoolParticipants({
  poolId,
  participantCount,
}: PoolParticipantsProps) {
  // Namespaced cursor: LP detail mounts PoolParticipants + PoolTransactions
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
  } else if (isError) {
    body = <QueryErrorState error={error} onRetry={() => void refetch()} />;
  } else if (rows.length === 0) {
    // The pool header's own count decides which fact this is: only a zero
    // count licenses "has no providers", since claiming that beside a non-zero
    // KPI on the same screen contradicts it (0377 F7).
    //
    // The warning is additionally gated on being the FIRST page. An empty page
    // behind a cursor is the ordinary end of the list, or a deep-linked stale
    // cursor — `useCursorPagination` preserves a pasted `?cursor=` on mount —
    // and neither is a failure. `participantCount` also comes from an earlier
    // request than the rows, so it can lag a withdrawal by a moment; "reports"
    // rather than "has" keeps the sentence true when it does.
    body =
      participantCount === 0 ? (
        <EmptyState
          icon={<GroupIcon />}
          title="No participants yet"
          description="This pool currently has no active liquidity providers."
        />
      ) : cursor != null ? (
        <EmptyState icon={<GroupIcon />} title="No more participants" />
      ) : (
        <EmptyState
          icon={<GroupIcon />}
          variant="warning"
          title="Participants unavailable"
          description={`This pool reports ${formatInteger(
            participantCount
          )} active liquidity providers, but none could be listed.`}
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
