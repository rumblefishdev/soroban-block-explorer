import type { LedgerDetailResponse } from '@rumblefish/api-types';
import {
  Chip,
  formatInteger,
  IdentifierWithCopy,
  TableSectionHeader,
} from '@rumblefish/soroban-block-explorer-ui';
import { Box, Card, Typography } from '@mui/material';
import type { ReactNode } from 'react';

import { FeeCell } from '../detail/FeeCell.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

import { TransactionCounts } from './TransactionCounts.js';

interface LedgerSummaryProps {
  ledger: LedgerDetailResponse;
}

interface SummaryCell {
  label: string;
  value: ReactNode;
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
            {formatInteger(ledger.sequence)}
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
      {
        label: 'Base fee',
        value: (
          <FeeCell
            stroops={ledger.base_fee}
            primaryVariant="bodySmBold"
            secondaryVariant="bodyMonoXsRegular"
          />
        ),
      },
    ],
    [
      {
        label: 'Transactions',
        value: (
          <TransactionCounts
            total={ledger.transaction_count}
            successful={ledger.successful_transaction_count}
          />
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
