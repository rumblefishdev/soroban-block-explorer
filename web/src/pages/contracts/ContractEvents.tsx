import { Box, Typography } from '@mui/material';
import type { PaginatedEventItem } from '@rumblefish/api-types';
import {
  Chip,
  type ChipProps,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  IdentifierDisplay,
  PaginationControls,
  QueryErrorState,
  TableEmptyState,
  truncateMiddle,
  useCursorPagination,
  usePageHandlers,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import { useMemo, type ReactNode } from 'react';

import { useContractEvents } from '../../api/index.js';
import { capitalize } from '../../utils/text.js';
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
  const label = type.length > 0 ? capitalize(type) : 'Unknown';
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
      sx={(theme) => ({
        display: 'block',
        maxWidth: 380,
        overflow: 'hidden',
        textOverflow: 'ellipsis',
        whiteSpace: 'nowrap',
        color: theme.palette.text.secondary,
      })}
    >
      [
      {topics.map((topic, index) => (
        <Box component="span" key={index}>
          {index > 0 && ', '}
          {typeof topic === 'string' ? (
            <Box
              component="span"
              sx={(theme) => ({ color: theme.palette.text.success })}
            >
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
      sx={(theme) => ({
        display: 'block',
        maxWidth: 260,
        overflow: 'hidden',
        textOverflow: 'ellipsis',
        whiteSpace: 'nowrap',
        color: theme.palette.text.primary,
      })}
    >
      {display}
    </Typography>
  );
}

const columns: ExplorerTableColumn<EventRow>[] = [
  {
    id: 'type',
    header: 'Type',
    width: 120,
    cell: (row) => <EventTypeBadge type={row.event_type} />,
  },
  {
    id: 'topics',
    header: 'Topics',
    width: 200,
    cell: (row) => <TopicsCell topics={row.topics} />,
  },
  {
    id: 'data',
    header: 'Data',
    width: 200,
    cell: (row) => <DataCell data={row.data} />,
  },
  {
    id: 'ledger',
    header: 'Ledger',
    width: 120,
    cell: (row) => (
      <IdentifierDisplay value={String(row.ledger_sequence)} type="ledger" />
    ),
  },
  {
    id: 'time',
    header: 'Time',
    width: 210,
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

  const { data, isLoading, isPlaceholderData, isError, error, refetch } =
    useContractEvents(contractId, cursor);

  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );

  let body: ReactNode;
  if (isLoading || isPlaceholderData) {
    body = (
      <ExplorerTable
        columns={columns}
        rows={[]}
        rowKey={(row, index) => `${row.transaction_hash}-${index}`}
        loading
        skeletonRows={20}
        rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      />
    );
  } else if (isError) {
    body = <QueryErrorState error={error} onRetry={() => void refetch()} />;
  } else if (rows.length === 0) {
    body = (
      <TableEmptyState
        kind="transactions"
        title="No events"
        description="This contract has not emitted any events yet."
        py={6}
      />
    );
  } else {
    body = (
      <ExplorerTable
        columns={columns}
        rows={rows}
        rowKey={(row, index) => `${row.transaction_hash}-${index}`}
        rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      />
    );
  }

  return (
    <Box>
      {body}
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
