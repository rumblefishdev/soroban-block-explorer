import type { XdrEventDto } from '@rumblefish/api-types';
import {
  Box,
  Collapse,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  Typography,
} from '@mui/material';
import { Chip, IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';
import { useState } from 'react';

import { SectionCard } from '../../detail/SectionCard.js';
import { DisclosureRow } from '../shared/DisclosureRow.js';

import { symTopic } from '../op-card/ExecutionTrace.js';
import { HighlightedJson } from '../op-card/HighlightedJson.js';

interface EventsSectionProps {
  /** The consensus stream: `contract` + `system`, tx-level and per-operation. */
  contractEvents: XdrEventDto[];
  /** The host's debug channel — see the diagnostics disclosure below. */
  diagnosticEvents: XdrEventDto[];
}

/** CAP-67 stage → the plain-English "when". Only tx-level events carry one. */
const STAGE_LABEL: Record<string, string> = {
  before_all_txs: 'before all txs',
  after_tx: 'after tx',
  after_all_txs: 'after all txs',
};

/** Where an event sits in the transaction, in the protocol's own terms: the
 *  operation that raised it (CAP-67 per-operation container, what `getEvents`
 *  calls `operationIndex`), or the ledger-application stage for a tx-level
 *  event. This is the honest answer to "why is the refund listed before the
 *  transfer" — the row number is a position in the record, the stage is the
 *  time. */
function whereLabel(event: XdrEventDto): string {
  if (event.op_index != null) return `op ${event.op_index + 1}`;
  if (event.stage != null) return STAGE_LABEL[event.stage] ?? event.stage;
  return '—';
}

function eventChip(event: XdrEventDto) {
  if (event.event_type === 'system') {
    return <Chip size="sm" color="violet" label="System" />;
  }
  if (event.event_type === 'contract') {
    return <Chip size="sm" color="blue" label="Contract" />;
  }
  return <Chip size="sm" color="neutral" label="Diagnostic" />;
}

/** Host resource counters (instructions, memory, ledger reads) — the bulk of
 *  the debug channel and the least of its meaning. The execution trace drops
 *  them for the same reason. */
function isHostCounter(event: XdrEventDto): boolean {
  return symTopic(event, 0) === 'core_metrics';
}

function EventTable({
  events,
  showWhere,
}: {
  events: readonly XdrEventDto[];
  showWhere: boolean;
}) {
  return (
    <Box sx={{ overflowX: 'auto' }}>
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell sx={{ width: 56 }}>#</TableCell>
            <TableCell sx={{ width: 130 }}>Type</TableCell>
            {showWhere && <TableCell sx={{ width: 120 }}>Where</TableCell>}
            <TableCell sx={{ width: 200 }}>Contract</TableCell>
            <TableCell>Topics</TableCell>
            <TableCell>Data</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {events.map((event) => (
            <TableRow key={event.event_index}>
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
                {eventChip(event)}
              </TableCell>
              {showWhere && (
                <TableCell
                  sx={(theme) => ({
                    verticalAlign: 'top',
                    color: theme.palette.text.tertiary,
                  })}
                >
                  {whereLabel(event)}
                </TableCell>
              )}
              <TableCell sx={{ verticalAlign: 'top' }}>
                {event.contract_id != null ? (
                  <IdentifierDisplay
                    value={event.contract_id}
                    type="contract"
                  />
                ) : (
                  <Typography
                    component="span"
                    sx={(theme) => ({ color: theme.palette.text.tertiary })}
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
  );
}

/**
 * The transaction's events, in the two channels the protocol actually defines.
 *
 * `contractEvents` is the consensus stream — `contract` + `system` events from
 * the tx-level and per-operation containers, hashed into the ledger. It is
 * what CAP-67 and `getEvents` mean by the events of a transaction, and it is
 * the count this card advertises.
 *
 * `diagnosticEvents` is the host's debug channel. It is not hashed, `getEvents`
 * does not return it, and with diagnostic mode on — always, for the archive we
 * read — it carries a byte-identical COPY of every consensus event alongside
 * the call trace and the resource counters. Merging the two into one list, as
 * this section used to, therefore printed the same transfer twice and inflated
 * the headline count (measured: 100 % copy rate over 394 342 V4 transactions,
 * task 0182). They are one channel about the other, not a continuation of it,
 * so they render as two.
 */
export function EventsSection({
  contractEvents,
  diagnosticEvents,
}: EventsSectionProps) {
  // Collapsed by default since this section is on the one-and-only view now
  // (0453 wave 5) — a fully expanded raw-JSON table is a wall of pixels.
  // Typed/humanised event rendering is task 0363.
  const [open, setOpen] = useState(false);
  const [diagOpen, setDiagOpen] = useState(false);
  const [hideCounters, setHideCounters] = useState(false);

  const counters = diagnosticEvents.filter(isHostCounter).length;
  const diagShown = hideCounters
    ? diagnosticEvents.filter((e) => !isHostCounter(e))
    : diagnosticEvents;

  const total = contractEvents.length;
  const plural = (n: number, word: string) =>
    `${n} ${word}${n === 1 ? '' : 's'}`;

  return (
    <SectionCard title="Events" meta={plural(total, 'event')}>
      {total === 0 ? (
        <Box sx={{ px: 2, py: 3 }}>
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
            label={`${open ? 'Hide' : 'Show'} ${plural(total, 'event')}`}
            sx={{ px: 2, py: 1.25 }}
          />
          <Collapse in={open} unmountOnExit>
            <EventTable events={contractEvents} showWhere />
          </Collapse>
        </>
      )}

      {diagnosticEvents.length > 0 && (
        <>
          <DisclosureRow
            open={diagOpen}
            onToggle={() => setDiagOpen((v) => !v)}
            label={`${diagOpen ? 'Hide' : 'Show'} execution diagnostics (${
              diagnosticEvents.length
            })`}
            sx={(theme) => ({
              px: 2,
              py: 1.25,
              borderTop: `1px solid ${theme.palette.stroke.default}`,
            })}
          />
          <Collapse in={diagOpen} unmountOnExit>
            <Box sx={{ px: 2, pb: 1.25 }}>
              <Typography
                variant="bodySmRegular"
                sx={(theme) => ({ color: theme.palette.text.tertiary })}
              >
                The host's debug channel — the call trace, resource counters,
                and a copy of each event above. Not hashed into consensus and
                not part of the transaction's event stream.
              </Typography>
            </Box>
            {counters > 0 && (
              <Box sx={{ px: 2, pb: 1.25 }}>
                <Chip
                  size="sm"
                  clickable
                  color={hideCounters ? 'neutral' : 'accent'}
                  aria-pressed={!hideCounters}
                  onClick={() => setHideCounters((v) => !v)}
                  label={`Host counters ${counters}`}
                />
              </Box>
            )}
            <EventTable events={diagShown} showWhere={false} />
          </Collapse>
        </>
      )}
    </SectionCard>
  );
}
