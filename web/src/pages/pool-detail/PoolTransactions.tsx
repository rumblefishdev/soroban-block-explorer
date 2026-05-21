import ListAltIcon from '@mui/icons-material/ListAlt';
import { Box, Stack, Typography } from '@mui/material';
import type { PoolTransactionItem } from '@rumblefish/api-types';
import {
  Chip,
  type ChipProps,
  classifyError,
  EmptyState,
  ExplorerTable,
  GenericErrorState,
  IdentifierWithCopy,
  PaginationControls,
  RateLimitState,
  RelativeTimestamp,
  TableSkeleton,
  TransientErrorState,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { usePoolTransactions } from '../../api/index.js';
import { SectionCard } from '../detail/SectionCard.js';
import { formatAbsoluteUtc } from '../transactions/formatters.js';
import { useInfinitePager } from '../useInfinitePager.js';

/**
 * Classifies a pool-touching transaction into the three Figma-defined
 * categories from its `operation_types[]` (server-side projected by
 * `20_get_liquidity_pools_transactions.sql`).
 *
 * Conflict policy: when a transaction carries both an LP-management op
 * (deposit / withdraw) and a path-payment trade against the same pool —
 * a rare bundling — the LP-management classification wins. Most
 * realistic transactions have a single LP-relevant operation.
 *
 * **Hard-fail on unknown ops.** If none of the recognised op kinds is
 * present we throw rather than render a silent fallback chip. The
 * `SectionErrorBoundary` around the transactions section will catch the
 * throw and surface a visible error state, so the gap can't ship
 * unnoticed if the backend ever starts returning a new op kind.
 */
function classifyLpTx(operationTypes: readonly string[]): {
  label: string;
  color: ChipProps['color'];
} {
  const has = (name: string) => operationTypes.includes(name);
  if (has('liquidity_pool_deposit'))
    return { label: 'Deposit', color: 'emerald' };
  if (has('liquidity_pool_withdraw'))
    return { label: 'Withdrawal', color: 'brown' };
  // Only path_payment_strict_* actually touch the pool's reserves; a
  // standalone manage_*_offer creates / updates a classic DEX offer
  // and doesn't move pool liquidity (the pool-tx endpoint can still
  // surface such tx if a *separate* op in the same tx touched the
  // pool, but in that case the path-payment branch above will fire).
  // Classifying on manage_*_offer would over-label as Trade.
  if (has('path_payment_strict_send') || has('path_payment_strict_receive'))
    return { label: 'Trade', color: 'blue' };
  throw new Error(
    `classifyLpTx: no recognised LP op kind in operation_types=[${operationTypes.join(
      ', '
    )}]`
  );
}

const columns: ExplorerTableColumn<PoolTransactionItem>[] = [
  {
    id: 'type',
    header: 'Type',
    cell: (row) => {
      const { label, color } = classifyLpTx(row.operation_types);
      return <Chip size="sm" color={color} label={label} />;
    },
  },
  {
    id: 'hash',
    header: 'Hash',
    cell: (row) => <IdentifierWithCopy value={row.hash} type="transaction" />,
  },
  {
    id: 'account',
    header: 'Account',
    cell: (row) => (
      <IdentifierWithCopy value={row.source_account} type="account" />
    ),
  },
  {
    id: 'time',
    header: 'Time',
    cell: (row) => (
      <Stack spacing={0}>
        <RelativeTimestamp timestamp={row.created_at} />
        <Typography variant="bodyXsRegular" sx={{ color: 'text.tertiary' }}>
          {formatAbsoluteUtc(row.created_at)}
        </Typography>
      </Stack>
    ),
  },
];

interface PoolTransactionsProps {
  poolId: string;
}

/**
 * "Recent transactions" section on the LP detail page. Columns:
 * Type (badge) / Hash / Account / Time. The Figma "Amount" column is
 * intentionally omitted in the MVP — backend per-tx LP amounts require
 * an XDR archive read-time fetch (ADR 0029) whose latency is under
 * research in task 0247. The FE add-back is task 0249.
 */
export function PoolTransactions({ poolId }: PoolTransactionsProps) {
  const {
    data,
    isLoading,
    isError,
    error,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
    refetch,
  } = usePoolTransactions(poolId);

  const { rows, canPrev, canNext, handlePrev, handleNext } = useInfinitePager(
    data?.pages ?? [],
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage
  );

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
          icon={<ListAltIcon />}
          title="No transactions yet"
          description="Activity for this pool will appear here once a deposit, withdrawal, or trade is recorded."
        />
      </Box>
    );
  } else {
    body = (
      <ExplorerTable columns={columns} rows={rows} rowKey={(row) => row.hash} />
    );
  }

  return (
    <SectionCard title="Recent transactions">
      <Box sx={{ minHeight: 280 }}>{body}</Box>
      <PaginationControls
        caption="Latest results"
        prevCursor={canPrev ? 'prev' : null}
        nextCursor={canNext ? 'next' : null}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </SectionCard>
  );
}
