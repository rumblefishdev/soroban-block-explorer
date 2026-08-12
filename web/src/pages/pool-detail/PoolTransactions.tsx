import ListAltIcon from '@mui/icons-material/ListAlt';
import { Stack, Typography } from '@mui/material';
import type { PoolItem, PoolTransactionItem } from '@rumblefish/api-types';
import {
  Chip,
  type ChipProps,
  EmptyState,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  formatTokenAmount,
  IdentifierWithCopy,
  PaginationControls,
  QueryErrorState,
  RelativeTimestamp,
  useCursorPagination,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { useMemo } from 'react';

import { usePagedRows, usePoolTransactions } from '../../api/index.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { SectionCard } from '../detail/SectionCard.js';
import { assetLegLabel } from '../pool-shared/helpers.js';
import { hashColumn } from '../transactions/cells.js';
import { formatAbsoluteUtc } from '../transactions/formatters.js';

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
  // Backend `OperationType` enum is SCREAMING_SNAKE_CASE (matches the
  // Stellar XDR `OperationType` discriminator). Compare against that
  // form — earlier this file used lowercase strings and silently
  // hard-failed on every LP transaction (caught by 0251 regression).
  const has = (name: string) => operationTypes.includes(name);
  if (has('LIQUIDITY_POOL_DEPOSIT'))
    return { label: 'Deposit', color: 'emerald' };
  if (has('LIQUIDITY_POOL_WITHDRAW'))
    return { label: 'Withdrawal', color: 'brown' };
  // Only path_payment_strict_* actually touch the pool's reserves; a
  // standalone manage_*_offer creates / updates a classic DEX offer
  // and doesn't move pool liquidity (the pool-tx endpoint can still
  // surface such tx if a *separate* op in the same tx touched the
  // pool, but in that case the path-payment branch above will fire).
  // Classifying on manage_*_offer would over-label as Trade.
  if (has('PATH_PAYMENT_STRICT_SEND') || has('PATH_PAYMENT_STRICT_RECEIVE'))
    return { label: 'Trade', color: 'blue' };
  throw new Error(
    `classifyLpTx: no recognised LP op kind in operation_types=[${operationTypes.join(
      ', '
    )}]`
  );
}

/**
 * What a row moved through this pool, as one display string — or `null` when
 * the row carries no amounts at all.
 *
 * `amount_a` / `amount_b` are raw stroops **signed from the pool's side**:
 * positive = the asset entered the pool (task 0279). That sign is the whole
 * direction story. One leg in and one out is a swap, so it reads
 * `in → out` (`12,059.29 XLM → 38.5M KALE`); two legs pointing the same way
 * are a deposit (`+/+`) or a withdrawal (`-/-`) and read `X + Y`, with the
 * Event chip already saying which.
 *
 * Amounts stay STRINGS end to end — `formatTokenAmount` consumes them exactly,
 * while a leg above 2^53 stroops would lose digits as a number.
 *
 * `null` in means NOT KNOWN, never zero: rows older than the amount index have
 * no value and must render blank rather than as `0`.
 */
export function formatPoolAmount(
  row: Pick<PoolTransactionItem, 'amount_a' | 'amount_b'>,
  pool: Pick<PoolItem, 'asset_a' | 'asset_b'>
): string | null {
  const legs = (
    [
      [row.amount_a, pool.asset_a],
      [row.amount_b, pool.asset_b],
    ] as const
  ).flatMap(([amount, leg]) => {
    if (amount == null || amount === '') return [];
    // The sign is carried by the ordering and the separator, not the digits.
    const text = formatTokenAmount(
      amount.replace(/^-/, ''),
      assetLegLabel(leg)
    );
    return text == null ? [] : [{ incoming: !amount.startsWith('-'), text }];
  });
  if (legs.length === 0) return null;

  const swap = legs.length === 2 && legs[0].incoming !== legs[1].incoming;
  if (!swap) return legs.map((leg) => leg.text).join(' + ');
  // A swap reads from what entered the pool to what left it.
  const ordered = legs[0].incoming ? legs : [...legs].reverse();
  return ordered.map((leg) => leg.text).join(' → ');
}

function poolTxColumns(
  pool: PoolItem
): ExplorerTableColumn<PoolTransactionItem>[] {
  return [
    {
      id: 'event',
      header: 'Event',
      width: 120,
      cell: (row) => {
        const { label, color } = classifyLpTx(row.operation_types);
        return <Chip size="sm" color={color} label={label} />;
      },
    },
    {
      id: 'amount',
      header: 'Amount',
      width: 260,
      cell: (row) => {
        const text = formatPoolAmount(row, pool);
        // Blank, not a dash or a zero: the amount is unknown for rows older
        // than the index, and an em-dash would read as "nothing moved".
        return text == null ? null : (
          <Typography variant="bodySmRegular" component="span">
            {text}
          </Typography>
        );
      },
    },
    hashColumn<PoolTransactionItem>(),
    {
      id: 'account',
      header: 'Account',
      width: 160,
      cell: (row) => (
        <IdentifierWithCopy value={row.source_account} type="account" />
      ),
    },
    {
      id: 'time',
      header: 'Time',
      width: 210,
      cell: (row) => (
        <Stack spacing={0}>
          <RelativeTimestamp timestamp={row.created_at} />
          <Typography
            variant="bodyXsRegular"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            {formatAbsoluteUtc(row.created_at)}
          </Typography>
        </Stack>
      ),
    },
  ];
}

interface PoolTransactionsProps {
  poolId: string;
  pool: PoolItem;
}

/**
 * "Recent transactions" section on the LP detail page. Columns:
 * Event (badge) / Amount / Hash / Account / Time.
 *
 * The Amount column was hidden from the MVP until task 0279 (issue #371):
 * per-tx LP amounts were nowhere in the DB, since
 * `operations_appearances.amount` is a fold count and not a transfer amount.
 * They are now indexed per (operation, pool, asset) by the ingest-side
 * extraction task 0247 chose — the parser already resolved every claim atom
 * per pool and threw the attribution away. Rows older than that index have no
 * amounts until the historical re-parse lands, and render blank.
 */
export function PoolTransactions({ poolId, pool }: PoolTransactionsProps) {
  // Namespaced cursor: LP detail mounts PoolParticipants + PoolTransactions
  // simultaneously, so each section needs its own URL key. `resetKey`
  // drops the cursor when the user navigates to a different pool.
  const { cursor, goNext, goPrev } = useCursorPagination({
    cursorParam: CURSOR_PARAMS.POOL_TRANSACTIONS,
    resetKey: poolId,
  });

  const { data, isLoading, isPlaceholderData, isError, error, refetch } =
    usePoolTransactions(poolId, cursor);

  const { rows, canPrev, canNext, handlePrev, handleNext } = usePagedRows(
    data,
    goNext,
    goPrev
  );

  // The Amount cell needs the pool's legs, so the columns close over them.
  const columns = useMemo(() => poolTxColumns(pool), [pool]);

  let body: ReactNode;
  if (isLoading || isPlaceholderData) {
    body = (
      <ExplorerTable
        columns={columns}
        rows={[]}
        rowKey={(row) => row.hash}
        loading
        skeletonRows={20}
        rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      />
    );
  } else if (isError) {
    body = <QueryErrorState error={error} onRetry={() => void refetch()} />;
  } else if (rows.length === 0) {
    body = (
      <EmptyState
        icon={<ListAltIcon />}
        title="No transactions yet"
        description="Activity for this pool will appear here once a deposit, withdrawal, or trade is recorded."
      />
    );
  } else {
    body = (
      <ExplorerTable
        columns={columns}
        rows={rows}
        rowKey={(row) => row.hash}
        rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      />
    );
  }

  return (
    <SectionCard title="Recent transactions">
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
