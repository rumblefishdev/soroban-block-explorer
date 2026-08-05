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
  /** The host's debug channel, listed raw — see the section doc. */
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
 * The transaction's events — two records, kept apart instead of concatenated.
 *
 * `contractEvents` is the consensus stream: `contract` + `system` events from
 * the tx-level and per-operation containers, hashed into the ledger. It is what
 * CAP-67 and `getEvents` mean by the events of a transaction, so it alone is
 * the list and the count this card advertises.
 *
 * `diagnosticEvents` is the host's debug channel, shown raw under its own
 * disclosure. Not hashed, not returned by `getEvents`, so it is not counted as
 * events — but it is not trimmed for tidiness either. The copies stay, exactly
 * as the ledger carries them: the execution trace on the operation card is a
 * readable rendering of these rows (the Stellar docs say the container exists
 * for "building the contract call stack"), and this table is the unprocessed
 * original it is derived from.
 *
 * The single exception is `core_metrics`. Those render in full one card up,
 * under Resources, and `readResourceCounters` is total — it cannot silently
 * drop one — so nothing leaves the page by omitting them here. What it buys is
 * that the four rows describing the execution are not buried under nineteen
 * rows of meter readings, which is the noise the issue reported.
 *
 * The bug in issue #378 was never that any of it was visible. It was that the
 * two records were CONCATENATED: one list, one count, so a transaction that
 * emitted three events advertised twenty-seven and printed its transfer twice.
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

  const total = contractEvents.length;
  // `core_metrics` is the one thing this table leaves out, and only because
  // the Resources disclosure on the operation card renders every one of them
  // in full (`readResourceCounters` is total — it cannot drop a counter). On
  // a minimal Soroban transaction they are 19 of 24 rows, all saying the same
  // kind of thing, and they would bury the four that describe the execution.
  const diagnostics = diagnosticEvents.filter(
    (e) => symTopic(e, 0) !== 'core_metrics'
  );
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

      {diagnostics.length > 0 && (
        <>
          <DisclosureRow
            open={diagOpen}
            onToggle={() => setDiagOpen((v) => !v)}
            label={`${diagOpen ? 'Hide' : 'Show'} ${
              diagnostics.length
            } diagnostic ${diagnostics.length === 1 ? 'entry' : 'entries'}`}
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
                The host's debug channel, raw: the call trace, contract logs,
                failure diagnostics, and byte-identical copies of the contract's
                own events above. Not hashed into consensus and not returned by{' '}
                <code>getEvents</code>, which is why it is kept out of the event
                count — not out of the page. The execution trace on the
                operation card is a readable view of these same rows; the{' '}
                <code>core_metrics</code> counters are the one thing listed only
                there, in full, under Resources.
              </Typography>
            </Box>
            {/* No `Where`: the debug channel states no position. What raised
                an entry is the trace's answer, and it is one card up. */}
            <EventTable events={diagnostics} showWhere={false} />
          </Collapse>
        </>
      )}
    </SectionCard>
  );
}
