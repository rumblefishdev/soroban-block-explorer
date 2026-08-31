import AddCircleOutline from '@mui/icons-material/AddCircleOutline';
import ListAltIcon from '@mui/icons-material/ListAlt';
import RemoveCircleOutline from '@mui/icons-material/RemoveCircleOutline';
import SwapHoriz from '@mui/icons-material/SwapHoriz';
import { MenuItem, Select, Stack, Typography } from '@mui/material';
import type {
  PoolActivityItem,
  PoolEvent,
  PoolItem,
} from '@rumblefish/api-types';
import {
  Chip,
  type ChipProps,
  IdentifierDisplay,
  EmptyState,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  formatAmount,
  IdentifierWithCopy,
  PaginationControls,
  QueryErrorState,
  RelativeTimestamp,
  scaleByDecimals,
  useCursorPagination,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ComponentType, ReactNode } from 'react';
import { Fragment, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';

import { usePagedRows, usePoolActivity } from '../../api/index.js';
import { AssetIcon } from '../assets/AssetIcon.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { SectionCard } from '../detail/SectionCard.js';
import { poolLegViews, type PoolLegView } from '../pool-shared/helpers.js';
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

/** The "no filter" entry. Empty string, matching `TransactionFilters`'
 *  `ALL_OPERATIONS` — the param is dropped from the URL rather than set to a
 *  sentinel the API would have to understand. */
const ALL_EVENTS = '';

/** URL param holding the active filter, so a filtered view is shareable —
 *  the reporter on issue #371 pointed at stellar.expert's `?filter=trades`. */
const EVENT_PARAM = 'event';

function isPoolEvent(value: string | null): value is PoolEvent {
  return value != null && (EVENT_FILTERS as string[]).includes(value);
}

/** One display leg of an operation's amount: the grouped decimal WITHOUT its
 *  unit (the unit renders separately as an icon + linked code), the leg's
 *  unified view, and its direction from the pool's side. `scaled` is the
 *  UNGROUPED decimal — the cross-world comparable value `tradeRate` divides
 *  (raw units are not comparable: legs scale by different `decimals`). */
export interface AmountLegPart {
  amount: string;
  scaled: string;
  view: PoolLegView;
  incoming: boolean;
}

/**
 * What ONE operation moved through this pool, as ordered display parts — or
 * `null` when it carries no readable leg.
 *
 * ONE arm for both worlds: rows normalize to `(leg index, signed raw
 * amount)` — soroban rows from `leg_amounts` (a pool can have 3–4 legs, a
 * trade touches two), classic rows from the `amount_a`/`amount_b` pair —
 * and `poolLegViews` supplies each leg's label/link/decimals uniformly.
 *
 * Amounts are raw units **signed from the pool's side**: positive = the
 * asset entered the pool. That sign is the whole direction story. One leg in
 * and one out is a swap (`swap: true`) and the parts come ordered from what
 * entered the pool to what left it; two legs pointing the same way are a
 * deposit or a withdrawal, joined with `+` and already named by the Event
 * chip.
 *
 * Amounts stay STRINGS end to end — `scaleByDecimals` is BigInt-exact, while
 * a leg above 2^53 raw units would lose digits as a number. A leg whose
 * scale is unknown is SKIPPED, never rendered as raw digits; a `null` leg
 * did not move in this operation — never rendered as `0`.
 */
export function poolAmountLegs(
  op: Pick<PoolActivityItem, 'amount_a' | 'amount_b' | 'leg_amounts'>,
  pool: PoolItem
): { legs: AmountLegPart[]; swap: boolean } | null {
  const views = poolLegViews(pool);
  const entries =
    op.leg_amounts?.map((la) => ({ i: la.leg_index, amount: la.amount })) ??
    [op.amount_a, op.amount_b].flatMap((amount, i) =>
      amount ? [{ i, amount }] : []
    );
  const legs = entries.flatMap(({ i, amount }) => {
    const view = views[i];
    if (view == null || view.decimals == null) return [];
    const scaled = scaleByDecimals(amount.replace(/^-/, ''), view.decimals);
    if (scaled == null) return [];
    return [
      {
        amount: formatAmount(scaled),
        scaled,
        view,
        incoming: !amount.startsWith('-'),
      },
    ];
  });
  if (legs.length === 0) return null;

  const swap = legs.length === 2 && legs[0].incoming !== legs[1].incoming;
  // A swap reads from what entered the pool to what left it.
  const ordered = swap && !legs[0].incoming ? [...legs].reverse() : legs;
  return { legs: ordered, swap };
}

/** The plain-text form of already-computed parts — the amount cell's
 *  `aria-label`, built from the same parts the cell renders. */
export function formatPoolAmountParts(
  parts: { legs: AmountLegPart[]; swap: boolean } | null
): string | null {
  if (parts == null) return null;
  return parts.legs
    .map((l) => `${l.amount} ${l.view.label}`)
    .join(parts.swap ? ' → ' : ' + ');
}

/** [`formatPoolAmountParts`] from a raw row — the shape the unit tests pin. */
export function formatPoolAmount(
  op: Pick<PoolActivityItem, 'amount_a' | 'amount_b' | 'leg_amounts'>,
  pool: PoolItem
): string | null {
  return formatPoolAmountParts(poolAmountLegs(op, pool));
}

/**
 * The execution rate of a swap, as `out per in` — `3,063 KALE/XLM` for a
 * trade that put XLM in and took KALE out, matching how stellar.expert quotes
 * it. `null` for anything that is not a two-legged swap, and for the
 * zero-amount dust edge (a rate against zero is not a number).
 *
 * Rounded to 4 significant figures. Doubles are fine HERE and only here: the
 * displayed amounts stay exact strings, and a relative error of 1e-16 cannot
 * move a 4-figure rate.
 */
export function tradeRate(
  parts: { legs: AmountLegPart[]; swap: boolean } | null
): string | null {
  if (parts == null || !parts.swap) return null;
  const [inLeg, outLeg] = parts.legs;
  const inValue = Number(inLeg.scaled);
  const outValue = Number(outLeg.scaled);
  if (!Number.isFinite(inValue) || !Number.isFinite(outValue) || inValue <= 0) {
    return null;
  }
  const rate = Number((outValue / inValue).toPrecision(4));
  const text = rate.toLocaleString('en-US', { maximumFractionDigits: 7 });
  return `${text} ${outLeg.view.label}/${inLeg.view.label}`;
}

/** Leg code as a link when the leg routes somewhere (native, classic credit,
 *  contract-id fallback — `legHref`'s documented precedence); plain text on
 *  schema drift. Same node the pools list and the pool summary render, so an
 *  asset reads and routes identically everywhere it appears. */
function assetCodeNode(view: PoolLegView): ReactNode {
  if (!view.href) return view.label;
  return (
    <IdentifierDisplay
      value={view.label}
      type="asset"
      truncate={false}
      href={view.href}
      fontSize="inherit"
    />
  );
}

/** Stable identity for a row. The hash is NOT unique here — a transaction
 *  running several operations (or emitting several pool events) appears once
 *  per row; the ledger disambiguates the soroban side, whose rows carry no
 *  `application_order`. */
export function activityRowKey(row: PoolActivityItem, index: number): string {
  return `${row.transaction_hash}-${row.application_order ?? `s${index}`}`;
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
      // One operation, one figure — no stack. The two-leg linked `A → B` form
      // is the widest case, which is what this width has to hold (carried over
      // from task 0490, whose line cap this row unit makes unreachable; was
      // 280 as plain text, the icons and link affordances buy 40px).
      width: 320,
      // Blank when there is nothing to show — not a dash and not a zero: a row
      // whose legs did not both land has no figure, and an em-dash would read
      // as "nothing moved".
      cell: (row) => {
        const parts = poolAmountLegs(row, pool);
        if (parts == null) return null;
        const rate = tradeRate(parts);
        const crossed = row.pools_crossed ?? 0;
        const sep = parts.swap ? '→' : '+';
        return (
          <Stack spacing={0.25}>
            <Stack
              direction="row"
              spacing={0.75}
              alignItems="center"
              aria-label={formatPoolAmountParts(parts) ?? undefined}
            >
              {parts.legs.map((l, i) => (
                <Fragment key={`${l.view.label}-${i}`}>
                  {i > 0 && (
                    <Typography
                      variant="bodySmRegular"
                      component="span"
                      sx={(theme) => ({ color: theme.palette.text.tertiary })}
                    >
                      {sep}
                    </Typography>
                  )}
                  <Typography variant="bodySmRegular" component="span">
                    {l.amount}
                  </Typography>
                  <AssetIcon
                    code={l.view.label}
                    iconUrl={l.view.iconUrl}
                    size={16}
                  />
                  {assetCodeNode(l.view)}
                </Fragment>
              ))}
            </Stack>
            {(rate != null || crossed > 1) && (
              <Stack direction="row" spacing={0.75} alignItems="center">
                {rate != null && (
                  <Typography
                    variant="bodyXsRegular"
                    component="span"
                    sx={(theme) => ({ color: theme.palette.text.tertiary })}
                  >
                    at {rate}
                  </Typography>
                )}
                {crossed > 1 && (
                  <Chip
                    size="sm"
                    color="blue"
                    label={`1 of ${crossed} pools`}
                  />
                )}
              </Stack>
            )}
          </Stack>
        );
      },
    },
    {
      id: 'operation',
      header: 'Operation',
      width: 190,
      // Links to the operation, not merely to its transaction: task 0482 gave
      // every operation a URL-addressable `#op-N` anchor on the detail page,
      // so a row about one operation can land on that operation. Soroban rows
      // carry no anchor (contract events are not operations) — plain tx link.
      cell: (row) => (
        <IdentifierWithCopy
          value={row.transaction_hash}
          type="transaction"
          href={`/transactions/${row.transaction_hash}${
            row.application_order != null ? `#op-${row.application_order}` : ''
          }`}
        />
      ),
    },
    {
      id: 'account',
      header: 'Account',
      width: 160,
      // First column off the boat on a phone: a truncated G-string is the
      // least scannable cell here, and the op detail page one tap away names
      // it anyway. Dropping it narrows the table for real — `hideBelow` also
      // removes its share of the scroll width.
      hideBelow: 'sm',
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
            sx={(theme) => ({
              color: theme.palette.text.tertiary,
              // The relative time carries the story on a phone; the exact
              // UTC stamp is a desktop affordance.
              display: { xs: 'none', sm: 'block' },
            })}
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

  const onFilter = (next: string) => {
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (next === ALL_EVENTS) params.delete(EVENT_PARAM);
        else params.set(EVENT_PARAM, next);
        return params;
      },
      { replace: true }
    );
  };

  // A `Select` whose first entry clears the filter — the same control the
  // transactions list and the pool list use. An earlier cut of this used a
  // ToggleButtonGroup, which is used nowhere else as a filter and offered no
  // visible way back to "everything": clearing meant clicking the active
  // button a second time, which nothing on screen suggests.
  const filterControl = (
    <Select
      size="small"
      value={event ?? ALL_EVENTS}
      onChange={(e) => onFilter(e.target.value)}
      displayEmpty
      aria-label="Filter activity by event"
      sx={{ width: { xs: '100%', sm: 220 } }}
    >
      <MenuItem value={ALL_EVENTS}>All events</MenuItem>
      {EVENT_FILTERS.map((value) => (
        <MenuItem key={value} value={value}>
          {EVENT_META[value].label}
        </MenuItem>
      ))}
    </Select>
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
