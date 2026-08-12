import { formatInteger } from '@rumblefish/soroban-block-explorer-ui';
import { Box, Typography } from '@mui/material';

interface TransactionCountsProps {
  /** `transaction_count` — successful plus failed. */
  total: number;
  /** `successful_transaction_count`; `null` when the API had none to report. */
  successful: number | null | undefined;
}

function Count({ value, tone }: { value: number; tone: 'success' | 'error' }) {
  return (
    <Box sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.5 }}>
      <Box
        sx={(theme) => ({
          width: 6,
          height: 6,
          borderRadius: '50%',
          backgroundColor: theme.palette.text[tone],
        })}
      />
      <Typography
        component="span"
        variant="bodySmRegular"
        sx={(theme) => ({ color: theme.palette.text[tone] })}
      >
        {formatInteger(value)}
      </Typography>
    </Box>
  );
}

/**
 * Successful and failed transaction counts for one ledger — `● 280  ● 85`.
 *
 * Two absolute numbers rather than the percentages the original request asked
 * for: at ~450 transactions per ledger a percentage is noise, while adjacent
 * counts read at a glance. This also fits the existing column width.
 *
 * Falls back to the plain total when `successful` is absent. A missing
 * aggregate must never render as `0 successful` — that would claim a ledger
 * in which every transaction failed.
 */
export function TransactionCounts({
  total,
  successful,
}: TransactionCountsProps) {
  if (successful == null) {
    return (
      <Typography component="span" variant="bodySmRegular">
        {formatInteger(total)}
      </Typography>
    );
  }

  // Clamped because the two numbers come from different tables: `total` from
  // `ledgers`, `successful` from `transactions`. They agreed on every sampled
  // ledger, but a negative failed count would be a nonsense render if they
  // ever drift.
  const failed = Math.max(0, total - successful);

  return (
    <Box sx={{ display: 'inline-flex', alignItems: 'center', gap: 1.5 }}>
      <Count value={successful} tone="success" />
      <Count value={failed} tone="error" />
    </Box>
  );
}
