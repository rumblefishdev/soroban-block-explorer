import { Skeleton, Stack } from '@mui/material';
import type { ReactNode } from 'react';

import { SectionCard } from './SectionCard.js';

/**
 * Titled summary-card skeleton — a `SectionCard` (real title + optional meta,
 * same as the loaded section) with key/value row placeholders. Used by detail
 * skeletons so each section shows its real heading (e.g. "Summary",
 * "Balances", "Details") instead of a generic grey bar — matching the loaded
 * view. Body rows are placeholders (the values are data).
 */
export function SummarySkeleton({
  title,
  meta,
  rows = 4,
}: {
  title: ReactNode;
  meta?: ReactNode;
  rows?: number;
}) {
  return (
    <SectionCard title={title} meta={meta}>
      <Stack spacing={1.5} sx={{ p: 2 }}>
        {Array.from({ length: rows }).map((_, i) => (
          <Stack key={i} direction="row" justifyContent="space-between" gap={2}>
            <Skeleton variant="text" width="30%" />
            <Skeleton variant="text" width="55%" />
          </Stack>
        ))}
      </Stack>
    </SectionCard>
  );
}
