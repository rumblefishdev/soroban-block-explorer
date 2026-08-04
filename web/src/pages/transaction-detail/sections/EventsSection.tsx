import type { XdrEventDto } from '@rumblefish/api-types';
import {
  Box,
  Collapse,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  Typography,
} from '@mui/material';
import { Chip, IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';
import { useMemo, useState } from 'react';

import { SectionCard } from '../../detail/SectionCard.js';
import { DisclosureRow } from '../shared/DisclosureRow.js';

import { symTopic } from '../op-card/ExecutionTrace.js';
import { HighlightedJson } from '../op-card/HighlightedJson.js';

interface EventsSectionProps {
  contractEvents: XdrEventDto[];
  diagnosticEvents: XdrEventDto[];
}

type EventKind = 'contract' | 'system' | 'diagnostic' | 'core_metrics';

/** Order of the filter chips — loudest first, noise last. */
const KINDS: readonly EventKind[] = [
  'contract',
  'system',
  'diagnostic',
  'core_metrics',
];

const KIND_LABEL: Record<EventKind, string> = {
  contract: 'Contract',
  system: 'System',
  diagnostic: 'Diagnostic',
  core_metrics: 'Host counters',
};

/** Row chips only — the filter chips follow the house on/off pair (see below).
 *  `subtle` is deliberately NOT used: its fill is `surface.grayMain`, this
 *  card's own background (#272727 dark / #fff light), so it reads as no chip
 *  at all. Host counters share the diagnostic grey — they ARE diagnostics,
 *  and the label is what tells the subset apart. */
const KIND_COLOR: Record<EventKind, 'blue' | 'violet' | 'neutral'> = {
  contract: 'blue',
  system: 'violet',
  diagnostic: 'neutral',
  core_metrics: 'neutral',
};

interface MergedEvent {
  event: XdrEventDto;
  kind: EventKind;
}

/** Which container an event arrived in decides diagnostic-vs-consensus — the
 *  diagnostic container carries byte-identical Contract-typed COPIES of the
 *  consensus events, so classifying on `event_type` alone would render every
 *  effect twice (task 0182, mirrored in the API's `split_events`). Within the
 *  consensus container `event_type` is the honest label: a `system` event is
 *  the host's own (`executable_update` on a contract upgrade), not the
 *  contract's. */
function classify(event: XdrEventDto, fromDiagnostics: boolean): EventKind {
  if (fromDiagnostics) {
    return symTopic(event, 0) === 'core_metrics'
      ? 'core_metrics'
      : 'diagnostic';
  }
  return event.event_type === 'system' ? 'system' : 'contract';
}

export function EventsSection({
  contractEvents,
  diagnosticEvents,
}: EventsSectionProps) {
  const merged = useMemo<MergedEvent[]>(
    () =>
      [
        ...contractEvents.map((event) => ({
          event,
          kind: classify(event, false),
        })),
        ...diagnosticEvents.map((event) => ({
          event,
          kind: classify(event, true),
        })),
        // `event_index` is one monotonic sequence across all three XDR event
        // containers (`extract_events`), so this restores the transaction's
        // own numbering rather than imposing an order. NOT chronology: the
        // containers come tx-level → per-op → diagnostic, so the AfterTx fee
        // refund is numbered before the operations it refunds. That numbering
        // is still the record, which is why the kinds filter one list instead
        // of splitting it into per-kind groups (#378).
      ].sort((a, b) => a.event.event_index - b.event.event_index),
    [contractEvents, diagnosticEvents]
  );

  const counts = useMemo(() => {
    const out = new Map<EventKind, number>();
    for (const { kind } of merged) out.set(kind, (out.get(kind) ?? 0) + 1);
    return out;
  }, [merged]);

  // Nothing hidden at rest: this section is the raw record, so it opens as
  // one. Filtering is the reader's move, not ours.
  const [hidden, setHidden] = useState<ReadonlySet<EventKind>>(() => new Set());
  const toggle = (kind: EventKind) =>
    setHidden((prev) => {
      const next = new Set(prev);
      if (!next.delete(kind)) next.add(kind);
      return next;
    });

  const shown = merged.filter(({ kind }) => !hidden.has(kind));
  const hiddenCount = merged.length - shown.length;

  const total = merged.length;
  // Collapsed by default since this section is on the one-and-only view now
  // (0453 wave 5) — a fully expanded raw-JSON table is a wall of pixels.
  // Typed/humanised event rendering is task 0363.
  const [open, setOpen] = useState(false);
  return (
    <SectionCard
      title="Events"
      meta={`${total} event${total === 1 ? '' : 's'}`}
    >
      {total === 0 ? (
        <Box sx={{ p: 3 }}>
          <Typography
            variant="bodySmRegular"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            No events emitted.
          </Typography>
        </Box>
      ) : (
        <>
          <DisclosureRow
            open={open}
            onToggle={() => setOpen((v) => !v)}
            label={`${open ? 'Hide' : 'Show'} ${total} event${
              total === 1 ? '' : 's'
            }`}
            sx={{ px: 2, py: 1.25 }}
          />
          <Collapse in={open} unmountOnExit>
            <Stack
              direction="row"
              spacing={1}
              sx={{ px: 2, pb: 1.25, flexWrap: 'wrap', rowGap: 1 }}
            >
              {KINDS.filter((kind) => counts.has(kind)).map((kind) => {
                const off = hidden.has(kind);
                return (
                  // Same on/off pair as every other filter bar in the app
                  // (AccountsFilters, AssetFilters, ContractsFilters): swap
                  // the `color` prop, never patch the fill with `sx`. `sm`
                  // rather than their `lg` — this row sits inside a card,
                  // next to `sm` row chips, not on a page-level filter bar.
                  <Chip
                    key={kind}
                    size="sm"
                    clickable
                    color={off ? 'neutral' : 'accent'}
                    aria-pressed={!off}
                    onClick={() => toggle(kind)}
                    label={`${KIND_LABEL[kind]} ${counts.get(kind) ?? 0}`}
                  />
                );
              })}
              {hiddenCount > 0 && (
                <Typography
                  variant="bodySmRegular"
                  sx={(theme) => ({
                    color: theme.palette.text.tertiary,
                    alignSelf: 'center',
                  })}
                >
                  {hiddenCount} hidden
                </Typography>
              )}
            </Stack>
            {shown.length === 0 ? (
              <Box sx={{ px: 2, pb: 2 }}>
                <Typography
                  variant="bodySmRegular"
                  sx={(theme) => ({ color: theme.palette.text.tertiary })}
                >
                  Every event is filtered out — turn a kind back on above.
                </Typography>
              </Box>
            ) : (
              <Box sx={{ overflowX: 'auto' }}>
                <Table size="small">
                  <TableHead>
                    <TableRow>
                      {/* The index is the transaction's own event numbering, so
                        a filtered list shows its own gaps instead of looking
                        complete. */}
                      <TableCell sx={{ width: 56 }}>#</TableCell>
                      <TableCell sx={{ width: 130 }}>Type</TableCell>
                      <TableCell sx={{ width: 200 }}>Contract</TableCell>
                      <TableCell>Topics</TableCell>
                      <TableCell>Data</TableCell>
                    </TableRow>
                  </TableHead>
                  <TableBody>
                    {shown.map(({ event, kind }) => (
                      <TableRow key={`${kind}-${event.event_index}`}>
                        <TableCell
                          sx={(theme) => ({
                            verticalAlign: 'top',
                            color: theme.palette.text.tertiary,
                            fontVariantNumeric: 'tabular-nums',
                          })}
                        >
                          {event.event_index}
                        </TableCell>
                        <TableCell sx={{ verticalAlign: 'top' }}>
                          <Chip
                            size="sm"
                            color={KIND_COLOR[kind]}
                            label={KIND_LABEL[kind]}
                          />
                        </TableCell>
                        <TableCell sx={{ verticalAlign: 'top' }}>
                          {event.contract_id != null ? (
                            <IdentifierDisplay
                              value={event.contract_id}
                              type="contract"
                            />
                          ) : (
                            <Typography
                              component="span"
                              sx={(theme) => ({
                                color: theme.palette.text.tertiary,
                              })}
                            >
                              —
                            </Typography>
                          )}
                        </TableCell>
                        <TableCell sx={{ verticalAlign: 'top' }}>
                          <HighlightedJson value={event.topics} />
                        </TableCell>
                        <TableCell sx={{ verticalAlign: 'top' }}>
                          <HighlightedJson value={event.data} />
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </Box>
            )}
          </Collapse>
        </>
      )}
    </SectionCard>
  );
}
