import { Box, Stack, Typography } from '@mui/material';
import {
  CardSkeleton,
  getDefaultTruncation,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';
import { useParams } from 'react-router-dom';

import { PageBreadcrumb } from '../detail/PageBreadcrumb.js';
import { TableSectionSkeleton } from '../detail/TableSectionSkeleton.js';

/**
 * Loading skeleton for the account detail page — header (breadcrumb + title +
 * id) + the two summary cards, matching the loaded layout. Used as BOTH the
 * route Suspense fallback (phase A) and the page's own `isLoading` return
 * (phase B) so there's no jump at the chunk boundary. Reads the id from the
 * URL so the header is real even in the fallback.
 */
export function AccountDetailSkeleton() {
  const { accountId = '' } = useParams<{ accountId: string }>();
  return (
    <Stack spacing={3}>
      <Box>
        <PageBreadcrumb
          items={[
            { label: 'Account' },
            {
              label: truncateMiddle(accountId, getDefaultTruncation('account')),
            },
          ]}
        />
        <Typography variant="heading5SemiBold" component="h1">
          Account
        </Typography>
        <Typography
          variant="bodyMedium"
          sx={(theme) => ({
            color: theme.palette.text.secondary,
            wordBreak: 'break-all',
          })}
        >
          {accountId}
        </Typography>
      </Box>
      <CardSkeleton />
      <CardSkeleton />
      <TableSectionSkeleton title="Recent transactions" rows={8} columns={6} />
    </Stack>
  );
}
