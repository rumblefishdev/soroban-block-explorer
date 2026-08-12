import { formatInteger } from '@rumblefish/soroban-block-explorer-ui';
import { Stack, Typography } from '@mui/material';

interface TransactionCountsProps {
  /** `transaction_count` — successful plus failed. */
  total: number;
  /** `successful_transaction_count`; nullish when the API had none to report. */
  successful: number | null | undefined;
}

/**
 * Rate above which the failure line turns red. Measured on production
 * 2026-08-12: the per-ledger failure rate runs 13.9% (p05) → 26.5% (median) →
 * 53% (p95), so failures are this chain's steady state, not an incident.
 * Colouring every row red would make red mean nothing and turn the live home
 * widget into a wall of alarm; this threshold flags roughly the top 5%.
 */
const ALARMING_FAILURE_RATE = 0.5;

/**
 * Two-line Transactions cell: total on top, failure rate underneath.
 *
 * The total is the primary scan key — it is the magnitude a ledgers list
 * exists to answer and the only comparable number in the row — so it keeps the
 * single right edge the column had before, with `tabular-nums` because Satoshi
 * digits are proportional (`0` is 2.1x the width of `1`).
 *
 * The rate rather than a second count: it is the informative half (a ~4x swing
 * across ledgers) and it is what the original request asked for. Nothing here
 * is carried by colour alone — the text reads identically in greyscale, under
 * any colour-vision deficiency, and to a screen reader via the wrapper label.
 *
 * Mirrors `TransactionTime`, the other two-line cell in this table, so the row
 * height it already reserves (`EXPLORER_TABLE_ROW_HEIGHT_TALL`) covers this one
 * at no cost.
 */
export function TransactionCounts({
  total,
  successful,
}: TransactionCountsProps) {
  // Treat a split wider than the total as absent rather than clamping it: the
  // two numbers come from different tables, and a clamp would render an
  // impossible ledger as fact instead of admitting the split is untrustworthy.
  const hasSplit = successful != null && successful >= 0 && successful <= total;
  const failed = hasSplit ? total - successful : 0;
  const failureRate = hasSplit && total > 0 ? failed / total : 0;

  return (
    <Stack spacing={0.25} aria-label={describe(total, hasSplit ? failed : null)}>
      <Typography
        component="span"
        variant="bodySmMedium"
        sx={(theme) => ({
          color: theme.palette.text.primary,
          fontVariantNumeric: 'tabular-nums',
        })}
      >
        {formatInteger(total)}
      </Typography>
      <Typography
        component="span"
        variant="bodyXsRegular"
        sx={(theme) => ({
          color:
            failureRate > ALARMING_FAILURE_RATE
              ? theme.palette.text.error
              : theme.palette.text.tertiary,
          fontVariantNumeric: 'tabular-nums',
        })}
      >
        {hasSplit
          ? `${(failureRate * 100).toFixed(1)}% failed`
          : 'split unavailable'}
      </Typography>
    </Stack>
  );
}

/**
 * Screen-reader text. The second line is never left blank — an unlabelled total
 * sitting where a split normally renders would read as a successful count.
 */
function describe(total: number, failed: number | null): string {
  const transactions = `${formatInteger(total)} transactions`;
  if (failed === null) {
    return `${transactions}, success split unavailable`;
  }
  return `${transactions}, ${formatInteger(total - failed)} succeeded, ${formatInteger(failed)} failed`;
}
