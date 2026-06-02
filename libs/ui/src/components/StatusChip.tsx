import { Chip } from './Chip.js';

/**
 * Success / failed status chip, shared across the transaction tables, the
 * transaction-detail summary header, and global search results. A single
 * boolean drives both the colour and the label.
 */
export function StatusChip({ successful }: { successful: boolean }) {
  return (
    <Chip
      size="sm"
      color={successful ? 'success' : 'error'}
      dot
      label={successful ? 'Success' : 'Failed'}
    />
  );
}
