import type { E3ResponseTransactionDetailLight } from '@rumblefish/api-types';
import { Stack, Typography } from '@mui/material';
import {
  Chip,
  formatFee,
  formatStroops,
  IdentifierDisplay,
  IdentifierWithCopy,
  RelativeTimestamp,
} from '@rumblefish/soroban-block-explorer-ui';

import { SectionCard } from '../../detail/SectionCard.js';
import { SummaryRow } from '../../detail/SummaryRow.js';
import { describeMemo } from '../shared/describeMemo.js';

interface TransactionSummaryProps {
  tx: E3ResponseTransactionDetailLight;
}

function Dash() {
  return (
    <Typography
      component="span"
      sx={(theme) => ({ color: theme.palette.text.tertiary })}
    >
      —
    </Typography>
  );
}

function MemoCell({
  memoType,
  memo,
}: {
  memoType: string | null | undefined;
  memo: string | null | undefined;
}) {
  const { typeLabel, content } = describeMemo(memoType, memo);
  if (typeLabel == null) return <Dash />;
  return (
    <Stack direction="row" spacing={1} alignItems="center" sx={{ minWidth: 0 }}>
      <Typography
        variant="bodySmRegular"
        sx={(theme) => ({
          color: theme.palette.text.primary,
          minWidth: 0,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          whiteSpace: 'nowrap',
        })}
      >
        {content || '—'}
      </Typography>
      <Chip size="sm" color="neutral" label={typeLabel} />
    </Stack>
  );
}

function pad(value: number): string {
  return value.toString().padStart(2, '0');
}

function formatUtcAbsolute(value: string): string | null {
  const d = new Date(value);
  if (!Number.isFinite(d.getTime())) return null;
  return (
    `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(
      d.getUTCDate()
    )} ` +
    `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(
      d.getUTCSeconds()
    )} UTC`
  );
}

function TimestampCell({ value }: { value: string }) {
  const absolute = formatUtcAbsolute(value);
  return (
    <Stack spacing={0.25} sx={{ minWidth: 0 }}>
      <RelativeTimestamp timestamp={value} />
      {absolute != null && (
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({ color: theme.palette.text.tertiary })}
        >
          {absolute}
        </Typography>
      )}
    </Stack>
  );
}

function FeeCell({ stroops }: { stroops: number }) {
  return (
    <Stack spacing={0.25} sx={{ minWidth: 0 }}>
      <Typography
        variant="bodySmRegular"
        sx={(theme) => ({ color: theme.palette.text.primary })}
      >
        {formatFee(stroops)}
      </Typography>
      <Typography
        variant="bodyXsRegular"
        sx={(theme) => ({ color: theme.palette.text.tertiary })}
      >
        ({formatStroops(stroops)} stroops)
      </Typography>
    </Stack>
  );
}

export function TransactionSummary({ tx }: TransactionSummaryProps) {
  const memoType = tx.heavy?.memo_type;
  const memo = tx.heavy?.memo;

  return (
    <SectionCard
      title={
        <Stack direction="row" spacing={1.5} alignItems="center">
          <Typography variant="heading5SemiBold" component="h2">
            Summary
          </Typography>
          <Chip
            size="sm"
            dot
            color={tx.successful ? 'success' : 'error'}
            label={tx.successful ? 'Success' : 'Failed'}
          />
        </Stack>
      }
    >
      <SummaryRow
        cells={[
          {
            label: 'Transaction hash',
            value: (
              <IdentifierWithCopy
                value={tx.hash}
                type="transaction"
                truncate={false}
              />
            ),
          },
        ]}
      />
      <SummaryRow
        cells={[
          {
            label: 'Ledger',
            value: (
              <IdentifierDisplay
                value={String(tx.ledger_sequence)}
                type="ledger"
              />
            ),
          },
          {
            label: 'Timestamp',
            value: <TimestampCell value={tx.created_at} />,
          },
        ]}
      />
      <SummaryRow
        cells={[
          { label: 'Fee charged', value: <FeeCell stroops={tx.fee_charged} /> },
          {
            label: 'Memo',
            value: <MemoCell memoType={memoType} memo={memo} />,
          },
        ]}
      />
      <SummaryRow
        cells={[
          {
            label: 'Source account',
            value:
              tx.source_account != null ? (
                <IdentifierWithCopy
                  value={tx.source_account}
                  type="account"
                  truncate={false}
                />
              ) : (
                <Dash />
              ),
          },
        ]}
      />
    </SectionCard>
  );
}
