import type { XdrEventDto } from '@rumblefish/api-types';
import {
  Box,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  Typography,
} from '@mui/material';
import { Chip, truncateMiddle } from '@rumblefish/soroban-block-explorer-ui';
import { useMemo } from 'react';

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
                        event.kind === 'contract' ? 'Contract' : 'Diagnostic'
                      }
                    />
                  </TableCell>
                  <TableCell sx={{ verticalAlign: 'top' }}>
                    {event.contract_id != null ? (
                      <Typography
                        component="span"
                        variant="bodyMonoSmMedium"
                        sx={(theme) => ({ color: theme.palette.text.primary })}
                      >
                        {truncateMiddle(event.contract_id, {
                          prefix: 5,
                          suffix: 4,
                        })}
                      </Typography>
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
                    <HighlightedJson value={event.topics} compact />
                  </TableCell>
                  <TableCell sx={{ verticalAlign: 'top' }}>
                    <HighlightedJson value={event.data} compact />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </Box>
      )}
    </SectionCard>
  );
}
