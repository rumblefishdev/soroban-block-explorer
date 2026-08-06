import WarningAmberOutlinedIcon from '@mui/icons-material/WarningAmberOutlined';
import { Button } from '@mui/material';
import { EmptyState } from '@rumblefish/soroban-block-explorer-ui';

/**
 * Shown when transaction data fetched from the Stellar history archive is
 * missing. Signatures, events and raw XDR live only in that block, so
 * defaulting them to `[]` makes a section render a confident "0 / none" for
 * something nothing measured. "Could not load" and "there are none" are
 * different facts and must not share a rendering (task 0377 F1/F2).
 *
 * Callers must NOT gate this on `heavy == null` alone: the archive can answer
 * while an individual transaction's envelope is missing, in which case the
 * heavy block exists with empty fields.
 *
 * `onRetry` renders the house "Try again" affordance (`GenericErrorState`) —
 * the fetch is a transient S3 round-trip, so a refetch often succeeds.
 */
export function HeavyUnavailable({
  what,
  onRetry,
}: {
  what: string;
  onRetry?: () => void;
}) {
  return (
    <EmptyState
      icon={<WarningAmberOutlinedIcon />}
      variant="warning"
      title={`${what} unavailable`}
      description="This transaction's full data could not be read from the Stellar archive."
      py={4}
      action={
        onRetry != null ? (
          <Button variant="contained" onClick={onRetry}>
            Try again
          </Button>
        ) : undefined
      }
    />
  );
}
