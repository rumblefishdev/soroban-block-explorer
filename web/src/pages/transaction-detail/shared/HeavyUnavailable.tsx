import WarningAmberOutlinedIcon from '@mui/icons-material/WarningAmberOutlined';
import { EmptyState } from '@rumblefish/soroban-block-explorer-ui';

/**
 * Shown when the archive-gated `heavy` block is absent, i.e.
 * `heavy_fields_status === 'unavailable'`.
 *
 * Signatures, events and raw XDR live ONLY in `heavy`. Defaulting them to `[]`
 * makes the sections render a confident "0 / none", which for signatures is
 * impossible (every applied transaction carries at least one) and for the rest
 * asserts a count nothing measured. "Could not load" and "there are none" are
 * different facts and must not share a rendering (task 0377 F1/F2).
 */
export function HeavyUnavailable({ what }: { what: string }) {
  return (
    <EmptyState
      icon={<WarningAmberOutlinedIcon />}
      variant="warning"
      title={`${what} unavailable`}
      description="Heavy XDR fields could not be loaded for this transaction."
      py={4}
    />
  );
}
