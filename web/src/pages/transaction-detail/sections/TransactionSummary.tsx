import type { E3ResponseTransactionDetailLight } from '@rumblefish/api-types';
import { Stack, Typography } from '@mui/material';
import {
  Chip,
  Dash,
  IdentifierDisplay,
  IdentifierWithCopy,
  RelativeTimestamp,
  StatusChip,
} from '@rumblefish/soroban-block-explorer-ui';

import { FeeCell } from '../../detail/FeeCell.js';
import { SectionCard } from '../../detail/SectionCard.js';
import { SummaryRow } from '../../detail/SummaryRow.js';
import { formatAbsoluteUtc } from '../../transactions/formatters.js';
import { describeMemo } from '../shared/describeMemo.js';

interface TransactionSummaryProps {
  tx: E3ResponseTransactionDetailLight;
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

function TimestampCell({ value }: { value: string }) {
  return (
    <Stack spacing={0.25} sx={{ minWidth: 0 }}>
      <RelativeTimestamp timestamp={value} />
      <Typography
        variant="bodyXsRegular"
        sx={(theme) => ({ color: theme.palette.text.tertiary })}
      >
        {formatAbsoluteUtc(value)}
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
          <StatusChip successful={tx.successful} />
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
                linked={false}
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
      {(tx.heavy?.fee_bump_source != null || tx.inner_tx_hash != null) && (
        <SummaryRow
          cells={[
            {
              label: 'Fee source',
              value:
                tx.heavy?.fee_bump_source != null ? (
                  <IdentifierWithCopy
                    value={tx.heavy.fee_bump_source}
                    type="account"
                    truncate={false}
                  />
                ) : (
                  <Dash />
                ),
            },
            {
              label: 'Inner transaction',
              // Copy only, deliberately unlinked: inner hashes are not
              // indexed, so /transactions/{inner} would 404 (0359 plan #4).
              value:
                tx.inner_tx_hash != null ? (
                  <IdentifierWithCopy
                    value={tx.inner_tx_hash}
                    type="transaction"
                    linked={false}
                  />
                ) : (
                  <Dash />
                ),
            },
          ]}
        />
      )}
    </SectionCard>
  );
}
