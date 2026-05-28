import { Box, Typography } from '@mui/material';
import type { PaginatedEventItem } from '@rumblefish/api-types';
import {
  Chip,
  type ChipProps,
  classifyError,
  ExplorerTable,
  GenericErrorState,
  IdentifierDisplay,
  PaginationControls,
  RateLimitState,
  TableEmptyState,
  TableSkeleton,
  TransientErrorState,
  truncateMiddle,
  useCursorPagination,
  usePageHandlers,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import { useMemo, type ReactNode } from 'react';

import { useContractEvents } from '../../api/index.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

type EventRow = PaginatedEventItem['data'][number];

// Chip colour per `event_type`, matching the Figma events table: contract
// blue, system brown (amber/cream), diagnostic grey. `/contracts/:id/events`
// only ever returns `contract` and `system` (the diagnostic container is
// dropped server-side, task 0182) — `diagnostic` is mapped defensively.
const EVENT_TYPE_COLOR: Record<string, ChipProps['color']> = {
  contract: 'blue',
  system: 'brown',
  diagnostic: 'neutral',
};

/** Event-type chip — colour-coded by the event's `event_type`. */
function EventTypeBadge({ type }: { type: string }) {
  const color = EVENT_TYPE_COLOR[type] ?? 'neutral';
  const label =
    type.length > 0 ? type.charAt(0).toUpperCase() + type.slice(1) : 'Unknown';
  return <Chip size="sm" color={color} label={label} />;
}

/**
 * Topics cell — the event topic array rendered as syntax-highlighted JSON:
 * string values green (Figma), brackets and commas dimmed, long addresses
 * middle-truncated. Full raw array on hover.
 */
function TopicsCell({ topics }: { topics: readonly unknown[] }) {
  const full = useMemo(() => {
    try {
      return JSON.stringify(topics) ?? String(topics);
    } catch {
      return String(topics);
    }
  }, [topics]);
  return (
    <Typography
      component="span"
      variant="bodyMonoXsRegular"
      title={full}
      sx={{
        display: 'block',
        maxWidth: 380,
        overflow: 'hidden',
        textOverflow: 'ellipsis',
        whiteSpace: 'nowrap',
        color: 'text.secondary',
      }}
    >
      [
      {topics.map((topic, index) => (
        <Box component="span" key={index}>
          {index > 0 && ', '}
          {typeof topic === 'string' ? (
            <Box component="span" sx={{ color: 'text.success' }}>
              {`"${truncateMiddle(topic, { prefix: 4, suffix: 4 })}"`}
            </Box>
          ) : (
            JSON.stringify(topic) ?? String(topic)
          )}
        </Box>
      ))}
      ]
    </Typography>
  );
}

/** Data cell — the event payload as plain monospace, full value on hover. */
function DataCell({ data }: { data: unknown }) {
  const full = useMemo(() => {
    if (typeof data === 'string') return data;
    try {
      return JSON.stringify(data) ?? String(data);
    } catch {
      return String(data);
    }
  }, [data]);
  const display =
    typeof data === 'string' && data.length > 24
      ? truncateMiddle(data, { prefix: 10, suffix: 10 })
      : full;
  return (
    <Typography
      component="span"
      variant="bodyMonoXsRegular"
      title={full}
      sx={{
        display: 'block',
        maxWidth: 260,
        overflow: 'hidden',
        textOverflow: 'ellipsis',
        whiteSpace: 'nowrap',
        color: 'text.primary',
      }}
    >
      {display}
    </Typography>
  );
}

const columns: ExplorerTableColumn<EventRow>[] = [
  {
    id: 'type',
    header: 'Type',
    cell: (row) => <EventTypeBadge type={row.event_type} />,
  },
  {
    id: 'topics',
    header: 'Topics',
    cell: (row) => <TopicsCell topics={row.topics} />,
  },
  {
    id: 'data',
    header: 'Data',
    cell: (row) => <DataCell data={row.data} />,
  },
  {
    id: 'ledger',
    header: 'Ledger',
    cell: (row) => (
      <IdentifierDisplay value={String(row.ledger_sequence)} type="ledger" />
    ),
  },
  {
    id: 'time',
    header: 'Time',
    cell: (row) => <TransactionTime createdAt={row.created_at} />,
  },
];

/**
 * Events tab — a paginated table of the contract's emitted events. A single
 * appearance can expand to several rows, so a page may hold more than the
 * requested limit; pagination is cursor-driven and never derives counts.
 */
export function ContractEvents({ contractId }: { contractId: string }) {
  // Namespaced cursor: contract detail tabs between Events + Invocations,
  // so each tab needs its own URL key. `resetKey` drops the cursor when
  // the user navigates to a different contract.
  const { cursor, goNext, goPrev } = useCursorPagination({
    cursorParam: CURSOR_PARAMS.CONTRACT_EVENTS,
    resetKey: contractId,
  });

  const { data, isLoading, isError, error, refetch } = useContractEvents(
    contractId,
    cursor
  );

  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );

  let body: ReactNode;
  if (isLoading) {
    body = (
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={8} columns={columns.length} />
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
        <TableEmptyState
          kind="transactions"
          title="No events"
          description="This contract has not emitted any events yet."
        />
      </Box>
    );
  } else {
    body = (
      <ExplorerTable
        columns={columns}
        rows={rows}
        rowKey={(row, index) => `${row.transaction_hash}-${index}`}
      />
    );
  }

  return (
    <Box>
      <Box sx={{ minHeight: 280 }}>{body}</Box>
      <PaginationControls
        caption="Latest results"
        canPrev={canPrev}
        canNext={canNext}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </Box>
  );
}
