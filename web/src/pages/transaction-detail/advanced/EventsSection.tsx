import type { XdrEventDto } from '@rumblefish/api-types';
import KeyboardArrowRightIcon from '@mui/icons-material/KeyboardArrowRight';
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

import { HighlightedJson } from './HighlightedJson.js';

interface EventsSectionProps {
  contractEvents: XdrEventDto[];
  diagnosticEvents: XdrEventDto[];
}

type EventKind = 'contract' | 'diagnostic';

interface MergedEvent extends XdrEventDto {
  kind: EventKind;
}

export function EventsSection({
  contractEvents,
  diagnosticEvents,
}: EventsSectionProps) {
  const merged = useMemo<MergedEvent[]>(
    () => [
      ...contractEvents.map((e) => ({ ...e, kind: 'contract' as const })),
      ...diagnosticEvents.map((e) => ({ ...e, kind: 'diagnostic' as const })),
    ],
    [contractEvents, diagnosticEvents]
  );

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
          <Box
            role="button"
            tabIndex={0}
            aria-expanded={open}
            onClick={() => setOpen((v) => !v)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                setOpen((v) => !v);
              }
            }}
            sx={(theme) => ({
              display: 'flex',
              alignItems: 'center',
              gap: 0.75,
              px: 2,
              py: 1.25,
              cursor: 'pointer',
              color: theme.palette.text.secondary,
            })}
          >
            <KeyboardArrowRightIcon
              sx={{
                fontSize: 18,
                transform: open ? 'rotate(90deg)' : 'none',
                transition: 'transform 120ms ease',
              }}
            />
            <Typography variant="bodySmSemiBold" sx={{ color: 'inherit' }}>
              {open ? 'Hide' : 'Show'} {total} event{total === 1 ? '' : 's'}
            </Typography>
          </Box>
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
                  {merged.map((event) => (
                    <TableRow key={`${event.kind}-${event.event_index}`}>
                      <TableCell sx={{ verticalAlign: 'top' }}>
                        <Chip
                          size="sm"
                          color={event.kind === 'contract' ? 'blue' : 'neutral'}
                          label={
                            event.kind === 'contract'
                              ? 'Contract'
                              : 'Diagnostic'
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
