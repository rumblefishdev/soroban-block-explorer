import { TableSkeleton } from '@rumblefish/soroban-block-explorer-ui';

import { SectionCard } from './SectionCard.js';

interface TableSectionSkeletonProps {
  title: string;
  rows?: number;
  columns?: number;
}

/**
 * A titled table section in its loading state — `SectionCard` header + a
 * `TableSkeleton` body. Matches the real table sections (pool participants /
 * transactions, account / asset transaction lists) so the table reserves its
 * space and doesn't pop in below the cards on load.
 */
export function TableSectionSkeleton({
  title,
  rows = 6,
  columns = 5,
}: TableSectionSkeletonProps) {
  return (
    <SectionCard title={title}>
      <TableSkeleton rows={rows} columns={columns} />
    </SectionCard>
  );
}
