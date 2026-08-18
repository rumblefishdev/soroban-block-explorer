import AddCircleOutline from '@mui/icons-material/AddCircleOutline';
import ListAltIcon from '@mui/icons-material/ListAlt';
import RemoveCircleOutline from '@mui/icons-material/RemoveCircleOutline';
import SwapHoriz from '@mui/icons-material/SwapHoriz';
import {
  Stack,
  ToggleButton,
  ToggleButtonGroup,
  Typography,
} from '@mui/material';
import type {
  PoolActivityItem,
  PoolEvent,
  PoolItem,
} from '@rumblefish/api-types';
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
import type { ComponentType, ReactNode } from 'react';
import { useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';

import { usePagedRows, usePoolActivity } from '../../api/index.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { SectionCard } from '../detail/SectionCard.js';
import { assetLegLabel } from '../pool-shared/helpers.js';
import { formatAbsoluteUtc } from '../transactions/formatters.js';

/**
 * How each event reads. The server decides WHICH event a row is (from the
 * sign pair of its two legs); this table only decides how it looks, so there
 * is no classification logic left on the client — that was `classifyLpTx`,
 * retired with the per-transaction row unit in task 0491.
 *
 * `SwapHoriz` is the icon the transaction detail already gives a path payment
 * (`op-card/opIcon.tsx`), so a trade looks the same in both places. Deposit
 * and withdrawal deliberately do NOT reuse that file's `Waves`: it maps both
 * LP ops to one glyph, which is right where the subject is the operation type
 * and wrong here, where the subject is what happened to the pool and the
 * direction is the entire distinction.
 */
const EVENT_META: Record<
  PoolEvent,
  { label: string; color: ChipProps['color']; Icon: ComponentType }
> = {
  deposit: { label: 'Deposit', color: 'emerald', Icon: AddCircleOutline },
  withdrawal: {
    label: 'Withdrawal',
    color: 'brown',
    Icon: RemoveCircleOutline,
  },
  trade: { label: 'Trade', color: 'blue', Icon: SwapHoriz },
};

/** `filter[event]` values, in the order the control offers them. */
const EVENT_FILTERS: PoolEvent[] = ['trade', 'deposit', 'withdrawal'];

/** URL param holding the active filter, so a filtered view is shareable —
 *  the reporter on issue #371 pointed at stellar.expert's `?filter=trades`. */
const EVENT_PARAM = 'event';

function isPoolEvent(value: string | null): value is PoolEvent {
  return value != null && (EVENT_FILTERS as string[]).includes(value);
}

/**
 * What ONE operation moved through this pool, as a display string — or `null`
 * when it carries no readable leg.
 *
 * `amount_a` / `amount_b` are raw stroops **signed from the pool's side**:
 * positive = the asset entered the pool. That sign is the whole direction
 * story. One leg in and one out is a swap, so it reads `in → out`
 * (`12,059.29 XLM → 38.5M KALE`); two legs pointing the same way are a deposit
 * or a withdrawal and read `X + Y`, with the Event chip already saying which.
 *
 * Amounts stay STRINGS end to end — `formatTokenAmount` consumes them exactly,
 * while a leg above 2^53 stroops would lose digits as a number.
 *
 * A leg that is `null` did not move in this operation — never rendered as `0`.
 */
export function formatPoolAmount(
  op: Pick<PoolActivityItem, 'amount_a' | 'amount_b'>,
  pool: Pick<PoolItem, 'asset_a' | 'asset_b'>
): string | null {
  const legs = (
    [
      [op.amount_a, pool.asset_a],
      [op.amount_b, pool.asset_b],
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

/** Stable identity for a row. The hash is NOT unique here — a transaction
 *  running several operations against one pool appears once per operation. */
export function activityRowKey(row: PoolActivityItem): string {
  return `${row.transaction_hash}-${row.application_order}`;
}

function activityColumns(
  pool: PoolItem
): ExplorerTableColumn<PoolActivityItem>[] {
  return [
    {
      id: 'event',
      header: 'Event',
      width: 140,
      // Accurate by construction: the row IS one operation, so the chip has
      // exactly one thing to name. The mixed deposit-and-trade bundle that
      // made the old per-transaction chip lie now renders as two rows, each
      // correctly labelled.
      cell: (row) => {
        if (row.event == null) return null;
        const { label, color, Icon } = EVENT_META[row.event];
        return <Chip size="sm" color={color} label={label} icon={<Icon />} />;
      },
    },
    {
      id: 'amount',
      header: 'Amount',
      // One operation, one figure — no stack. The two-leg `A → B` form is the
      // widest case, which is what this width has to hold (carried over from
      // task 0490, whose line cap this row unit makes unreachable).
      width: 280,
      // Blank when there is nothing to show — not a dash and not a zero: a row
      // whose legs did not both land has no figure, and an em-dash would read
      // as "nothing moved".
      cell: (row) => {
        const text = formatPoolAmount(row, pool);
        return text == null ? null : (
          <Typography variant="bodySmRegular" component="span">
            {text}
          </Typography>
        );
      },
    },
    {
      id: 'operation',
      header: 'Operation',
      width: 190,
      // Links to the operation, not merely to its transaction: task 0482 gave
      // every operation a URL-addressable `#op-N` anchor on the detail page,
      // so a row about one operation can land on that operation.
      cell: (row) => (
        <IdentifierWithCopy
          value={row.transaction_hash}
          type="transaction"
          href={`/transactions/${row.transaction_hash}#op-${row.application_order}`}
        />
      ),
    },
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

interface PoolActivityProps {
  poolId: string;
  pool: PoolItem;
}

/**
 * "Recent activity" section on the LP detail page — one row per OPERATION
 * against this pool, with a trade / deposit / withdrawal filter (task 0491,
 * issue #371).
 *
 * The row used to be a transaction, which could not carry an honest Event
 * chip (a bundled deposit + trade collapsed to one label), forced the Amount
 * cell to stack figures that must not be summed, and made a trades filter
 * inexpressible — "trades only" has no truthful answer for a transaction that
 * both deposits and trades.
 */
export function PoolActivity({ poolId, pool }: PoolActivityProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const raw = searchParams.get(EVENT_PARAM);
  const event = isPoolEvent(raw) ? raw : undefined;

  // Namespaced cursor: the LP detail mounts several paged sections at once, so
  // each needs its own URL key. Changing pool OR filter drops the cursor — a
  // cursor minted under one filter names a position that does not exist under
  // another.
  const { cursor, goNext, goPrev } = useCursorPagination({
    cursorParam: CURSOR_PARAMS.POOL_ACTIVITY,
    resetKey: `${poolId}:${event ?? 'all'}`,
  });

  const { data, isLoading, isPlaceholderData, isError, error, refetch } =
    usePoolActivity(poolId, cursor, event);

  const { rows, canPrev, canNext, handlePrev, handleNext } = usePagedRows(
    data,
    goNext,
    goPrev
  );

  // The Amount cell needs the pool's legs, so the columns close over them.
  const columns = useMemo(() => activityColumns(pool), [pool]);

  const onFilter = (_: unknown, next: PoolEvent | null) => {
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (next == null) params.delete(EVENT_PARAM);
        else params.set(EVENT_PARAM, next);
        return params;
      },
      { replace: true }
    );
  };

  const filterControl = (
    <ToggleButtonGroup
      size="small"
      exclusive
      value={event ?? null}
      onChange={onFilter}
      aria-label="Filter activity by event"
    >
      {EVENT_FILTERS.map((value) => (
        <ToggleButton key={value} value={value}>
          {EVENT_META[value].label}
        </ToggleButton>
      ))}
    </ToggleButtonGroup>
  );

  let body: ReactNode;
  if (isLoading || isPlaceholderData) {
    body = (
      <ExplorerTable
        columns={columns}
        rows={[]}
        rowKey={activityRowKey}
        loading
        skeletonRows={20}
        rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      />
    );
  } else if (isError) {
    body = <QueryErrorState error={error} onRetry={() => void refetch()} />;
  } else if (rows.length === 0) {
    // A filtered empty result is a different fact from an empty pool, and
    // saying "no activity yet" under an active filter reads as a bug.
    body = event ? (
      <EmptyState
        icon={<ListAltIcon />}
        title={`No ${EVENT_META[event].label.toLowerCase()} activity`}
        description="This pool has activity, but none of this kind. Clear the filter to see everything."
      />
    ) : (
      <EmptyState
        icon={<ListAltIcon />}
        title="No activity yet"
        description="Activity for this pool will appear here once a deposit, withdrawal, or trade is recorded."
      />
    );
  } else {
    body = (
      <ExplorerTable
        columns={columns}
        rows={rows}
        rowKey={activityRowKey}
        rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      />
    );
  }

  return (
    <SectionCard title="Recent activity" action={filterControl}>
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
