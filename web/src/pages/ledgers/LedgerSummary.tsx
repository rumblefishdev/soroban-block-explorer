import type { LedgerDetailResponse } from '@rumblefish/api-types';
import {
  Chip,
  IdentifierWithCopy,
  TableSectionHeader,
} from '@rumblefish/soroban-block-explorer-ui';
import { Box, Card, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

import { formatFee } from '../transactions/formatters.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

interface LedgerSummaryProps {
  ledger: LedgerDetailResponse;
}

interface SummaryCell {
  label: string;
  value: ReactNode;
}

function BaseFee({ stroops }: { stroops: number }) {
  return (
    <Stack spacing={0.25}>
      <Typography
        variant="bodySmBold"
        sx={(theme) => ({ color: theme.palette.text.primary })}
      >
        {formatFee(stroops)}
      </Typography>
      <Typography
        variant="bodyMonoXsRegular"
        sx={(theme) => ({ color: theme.palette.text.tertiary })}
      >
        ({stroops.toLocaleString('en-US')} stroops)
      </Typography>
    </Stack>
  );
}

function Cell({ label, value }: SummaryCell) {
  return (
    <Box
      sx={{
        display: 'flex',
        flex: 1,
        minWidth: 0,
        gap: 2,
        px: 2,
        py: 1.5,

        alignItems: 'center',
      }}
    >
      <Typography
        variant="bodySmMedium"
        sx={(theme) => ({
          color: theme.palette.text.primary,
          width: { xs: 'auto', sm: 160 },
          flexShrink: 0,
        })}
      >
        {label}
      </Typography>
      <Box sx={{ minWidth: 0 }}>{value}</Box>
    </Box>
  );
}

function Row({ cells, alt }: { cells: SummaryCell[]; alt: boolean }) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        flexDirection: { xs: 'column', sm: 'row' },
        backgroundColor: alt
          ? theme.palette.surface.grayMainAlt
          : theme.palette.surface.grayMain,
      })}
    >
      {cells.map((cell) => (
        <Cell key={cell.label} {...cell} />
      ))}
    </Box>
  );
}

/** Ledger metadata rendered as a two-column key/value table. */
export function LedgerSummary({ ledger }: LedgerSummaryProps) {
  const rows: SummaryCell[][] = [
    [
      {
        label: 'Sequence',
        value: (
          <Typography
            variant="bodySmBold"
            sx={(theme) => ({ color: theme.palette.text.primary })}
          >
            {ledger.sequence.toLocaleString('en-US')}
          </Typography>
        ),
      },
      {
        label: 'Protocol version',
        value: (
          <Chip
            size="sm"
            color="neutral"
            label={String(ledger.protocol_version)}
          />
        ),
      },
    ],
    [
      {
        label: 'Hash',
        // Full ledger hash (detail view) — not linked: a ledger has no
        // hash route, it is addressed by sequence.
        value: (
          <IdentifierWithCopy
            value={ledger.hash}
            type="ledger"
            linked={false}
            truncate={false}
          />
        ),
      },
    ],
    [
      {
        label: 'Timestamp',
        value: <TransactionTime createdAt={ledger.closed_at} />,
      },
      { label: 'Base fee', value: <BaseFee stroops={ledger.base_fee} /> },
    ],
    [
      {
        label: 'TX Count',
        value: (
          <Typography
            variant="bodySmBold"
            sx={(theme) => ({ color: theme.palette.text.primary })}
          >
            {ledger.transaction_count.toLocaleString('en-US')}
          </Typography>
        ),
      },
    ],
  ];

  return (
    <Card>
      <TableSectionHeader title="Summary" />
      {rows.map((cells, index) => (
        <Row key={cells[0].label} cells={cells} alt={index % 2 === 1} />
      ))}
    </Card>
  );
}
