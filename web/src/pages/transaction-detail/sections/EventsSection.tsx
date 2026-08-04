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
import { useMemo, useState } from 'react';

import { SectionCard } from '../../detail/SectionCard.js';
import { DisclosureRow } from '../shared/DisclosureRow.js';
import { HeavyUnavailable } from '../shared/HeavyUnavailable.js';

import { HighlightedJson } from '../op-card/HighlightedJson.js';

interface EventsSectionProps {
  contractEvents: XdrEventDto[];
  diagnosticEvents: XdrEventDto[];
  /** `heavy` absent — events were never loaded, not proven absent. */
  unavailable?: boolean;
}

type EventKind = 'contract' | 'diagnostic';

interface MergedEvent {
  event: XdrEventDto;
  kind: EventKind;
}

export function EventsSection({
  contractEvents,
  diagnosticEvents,
  unavailable = false,
}: EventsSectionProps) {
  const merged = useMemo<MergedEvent[]>(
    () => [
      ...contractEvents.map((event) => ({ event, kind: 'contract' as const })),
      ...diagnosticEvents.map((event) => ({
        event,
        kind: 'diagnostic' as const,
      })),
    ],
    [contractEvents, diagnosticEvents]
  );

  const total = merged.length;
  // Collapsed by default since this section is on the one-and-only view now
  // (0453 wave 5) — a fully expanded raw-JSON table is a wall of pixels.
  // Typed/humanised event rendering is task 0363.
  const [open, setOpen] = useState(false);

  // After the hooks: an absent `heavy` means the events were never fetched, so
  // the count below would assert a zero nothing measured (0377 F2).
  if (unavailable) {
    return (
      <SectionCard title="Events">
        <HeavyUnavailable what="Events" />
      </SectionCard>
    );
  }

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
            <Box sx={{ overflowX: 'auto' }}>
              <Table size="small">
                <TableHead>
                  <TableRow>
                    <TableCell sx={{ width: 130 }}>Type</TableCell>
                    <TableCell sx={{ width: 200 }}>Contract</TableCell>
                    <TableCell>Topics</TableCell>
                    <TableCell>Data</TableCell>
                  </TableRow>
                </TableHead>
                <TableBody>
                  {merged.map(({ event, kind }) => (
                    <TableRow key={`${kind}-${event.event_index}`}>
                      <TableCell sx={{ verticalAlign: 'top' }}>
                        <Chip
                          size="sm"
                          color={kind === 'contract' ? 'blue' : 'neutral'}
                          label={
                            kind === 'contract' ? 'Contract' : 'Diagnostic'
                          }
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
          </Collapse>
        </>
      )}
    </SectionCard>
  );
}
