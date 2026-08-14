import { Box, Skeleton, Stack, Typography } from '@mui/material';
import {
  getDefaultTruncation,
  TableSkeleton,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';
import { PageBreadcrumb } from '../detail/PageBreadcrumb.js';
import { SectionCard } from '../detail/SectionCard.js';

import { useTxHashParam } from './useTxHashParam.js';

/** Key/value rows placeholder for a summary card body. */
function RowsSkeleton({ rows }: { rows: number }) {
  return (
    <Stack spacing={1.5} sx={{ p: 2 }}>
      {Array.from({ length: rows }).map((_, i) => (
        <Stack key={i} direction="row" justifyContent="space-between" gap={2}>
          <Skeleton variant="text" width="30%" />
          <Skeleton variant="text" width="55%" />
        </Stack>
      ))}
    </Stack>
  );
}

/**
 * Loading skeleton for the transaction detail page. Mirrors the real section
 * structure (header, Summary, Operations 2-col, Signatures) via
 * `SectionCard` so the skeleton looks like the page, not three generic cards.
 * Used as BOTH the route Suspense fallback (phase A) and the page's
 * `isLoading` return (phase B). Reads the hash from the URL for the breadcrumb.
 */
export function TransactionDetailSkeleton() {
  const { hash } = useTxHashParam();
  return (
    <Stack spacing={3}>
      <Box>
        <PageBreadcrumb
          items={[
            { label: 'Transactions', to: routes.transactions },
            {
              label: truncateMiddle(hash, getDefaultTruncation('transaction')),
            },
          ]}
        />
        <Typography variant="heading5SemiBold" component="h1">
          Transaction Detail
        </Typography>
      </Box>

      {/* Summary — title is "Summary" + status chip, same as loaded */}
      <SectionCard
        title={
          <Stack direction="row" spacing={1.5} alignItems="center">
            <Typography variant="heading5SemiBold" component="h2">
              Summary
            </Typography>
            <Skeleton variant="rounded" width={64} height={22} />
          </Stack>
        }
      >
        <RowsSkeleton rows={5} />
      </SectionCard>

      {/* Operations — meta count + [picker | card] in the live grid ratio
          (md 5/7, lg 4/8 — keep in sync with OperationsSection). */}
      <SectionCard
        title="Operations"
        meta={<Skeleton variant="text" width={80} />}
      >
        <Box
          sx={{
            p: 2,
            display: { xs: 'block', md: 'grid' },
            gridTemplateColumns: { md: '5fr 7fr', lg: '4fr 8fr' },
            gap: 2,
          }}
        >
          <Skeleton variant="rounded" height={320} />
          <Skeleton
            variant="rounded"
            height={320}
            sx={{ mt: { xs: 2, md: 0 } }}
          />
        </Box>
      </SectionCard>

      {/* Signatures */}
      <SectionCard
        title="Signatures"
        meta={<Skeleton variant="text" width={80} />}
      >
        <TableSkeleton rows={3} columns={4} />
      </SectionCard>

      {/* Events + Raw data — collapsed disclosure rows on the live page,
          so a single row-height ghost each. */}
      <SectionCard title="Events" meta={<Skeleton variant="text" width={80} />}>
        <Box sx={{ px: 2, py: 1.25 }}>
          <Skeleton variant="text" width={160} />
        </Box>
      </SectionCard>
      <SectionCard
        title="Raw data"
        meta={<Skeleton variant="text" width={80} />}
      >
        <Stack spacing={1} sx={{ px: 2, py: 1.25 }}>
          <Skeleton variant="text" width={200} />
          <Skeleton variant="text" width={200} />
        </Stack>
      </SectionCard>
    </Stack>
  );
}
