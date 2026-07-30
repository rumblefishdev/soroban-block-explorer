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

      {/* Operations — meta count + 2 equal cols [picker | panel] */}
      <SectionCard
        title="Operations"
        meta={<Skeleton variant="text" width={80} />}
      >
        <Box
          sx={{
            p: 2,
            display: { xs: 'block', md: 'grid' },
            gridTemplateColumns: { md: '1fr 1fr' },
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
    </Stack>
  );
}
