import { Box, Stack, Typography } from '@mui/material';
import {
  CardSkeleton,
  getDefaultTruncation,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';

import { PageBreadcrumb } from '../detail/PageBreadcrumb.js';

import { useTxHashParam } from './useTxHashParam.js';

/**
 * Loading skeleton for the transaction detail page. Used in BOTH places so
 * the lazy-chunk fallback (phase A) and the page's own data-loading state
 * (phase B) are byte-identical — no layout jump at the chunk boundary:
 *   - as the route Suspense fallback in `router/index.tsx`
 *   - as the `isLoading` return inside `TransactionDetailPage`
 *
 * Reads the hash from the URL itself (`useTxHashParam` works under the route
 * context, incl. in the fallback) so the breadcrumb shows the real id even
 * before the page chunk loads. Light + self-contained (no heavy sections) so
 * it stays out of the lazy chunk.
 */
export function TransactionDetailSkeleton() {
  const { hash } = useTxHashParam();
  return (
    <Stack spacing={3}>
      <Box>
        <PageBreadcrumb
          items={[
            { label: 'Transactions', to: '/transactions' },
            {
              label: truncateMiddle(hash, getDefaultTruncation('transaction')),
            },
          ]}
        />
        <Typography variant="heading5SemiBold" component="h1">
          Transaction Detail
        </Typography>
      </Box>
      <CardSkeleton />
      <CardSkeleton />
      <CardSkeleton />
    </Stack>
  );
}
