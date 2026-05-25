import GroupIcon from '@mui/icons-material/GroupOutlined';
import { Box, Typography } from '@mui/material';
import type { ParticipantItem } from '@rumblefish/api-types';
import {
  classifyError,
  EmptyState,
  ExplorerTable,
  GenericErrorState,
  IdentifierWithCopy,
  PaginationControls,
  RateLimitState,
  TableSkeleton,
  TransientErrorState,
  useCursorPagination,
  usePageHandlers,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { usePoolParticipants } from '../../api/index.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { SectionCard } from '../detail/SectionCard.js';
import { formatAmount } from '../format.js';

const columns: ExplorerTableColumn<ParticipantItem>[] = [
  {
    id: 'account',
    header: 'Account',
    cell: (row) => <IdentifierWithCopy value={row.account} type="account" />,
  },
  {
    id: 'shares',
    header: 'Shares',
    align: 'right',
    cell: (row) => (
      <Typography component="span" variant="bodySmRegular">
        {formatAmount(row.shares)}
      </Typography>
    ),
  },
  {
    id: 'share_percentage',
    header: 'Share %',
    align: 'right',
    cell: (row) => (
      <Typography component="span" variant="bodySmRegular">
        {row.share_percentage != null
          ? `${formatAmount(row.share_percentage, 2)}%`
          : '—'}
      </Typography>
    ),
  },
  {
    id: 'first_deposit_ledger',
    header: 'Since ledger',
    align: 'right',
    cell: (row) => (
      <Typography component="span" variant="bodySmRegular">
        {formatAmount(row.first_deposit_ledger)}
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
  // Namespaced cursor: LP detail mounts PoolParticipants + PoolTransactions
  // simultaneously, so each section needs its own URL key. `resetKey`
  // drops the cursor when the user navigates to a different pool.
  const { cursor, canPrev, goNext, goPrev } = useCursorPagination({
    cursorParam: CURSOR_PARAMS.POOL_PARTICIPANTS,
    resetKey: poolId,
  });

  const { data, isLoading, isError, error, refetch } = usePoolParticipants(
    poolId,
    cursor
  );

  const rows = data?.data ?? [];
  const { canNext, handleNext } = usePageHandlers(data?.page, goNext);

  let body: ReactNode;
  if (isLoading) {
    body = (
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={6} columns={columns.length} />
      </Box>
    );
  } else if (isError) {
    const kind = classifyError(error);
    const retry = () => void refetch();
    body = (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 6 }}>
        {kind === 'rate-limit' ? (
          <RateLimitState onRetry={retry} />
        ) : kind === 'transient' ? (
          <TransientErrorState onRetry={retry} />
        ) : (
          <GenericErrorState onRetry={retry} />
        )}
      </Box>
    );
  } else if (rows.length === 0) {
    body = (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 6 }}>
        <EmptyState
          icon={<GroupIcon />}
          title="No participants yet"
          description="This pool currently has no active liquidity providers."
        />
      </Box>
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
      <Box sx={{ minHeight: 280 }}>{body}</Box>
      <PaginationControls
        caption="Latest results"
        canPrev={canPrev}
        canNext={canNext}
        onPrev={goPrev}
        onNext={handleNext}
      />
    </SectionCard>
  );
}
