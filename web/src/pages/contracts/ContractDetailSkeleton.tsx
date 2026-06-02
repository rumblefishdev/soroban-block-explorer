import { Box, Stack, Typography } from '@mui/material';
import {
  CardSkeleton,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';
import { useParams } from 'react-router-dom';

import { routes } from '../../router/routes.js';
import { KpiStripSkeleton } from '../detail/KpiStripSkeleton.js';
import { PageBreadcrumb } from '../detail/PageBreadcrumb.js';

const BREADCRUMB_TRUNCATION = { prefix: 4, suffix: 4 } as const;

/**
 * Loading skeleton for the contract detail page — header + the summary card.
 * The tabbed section is gated on resolved data (not shown while loading), so
 * the skeleton omits it too. Used as BOTH route fallback (phase A) and the
 * page's `isLoading` return (phase B). Reads the id from the URL.
 */
export function ContractDetailSkeleton() {
  const { contractId = '' } = useParams<{ contractId: string }>();
  return (
    <Stack spacing={3}>
      <Box>
        <PageBreadcrumb
          items={[
            { label: 'Contracts', to: routes.contracts },
            { label: truncateMiddle(contractId, BREADCRUMB_TRUNCATION) },
          ]}
        />
        <Typography variant="heading5SemiBold" component="h1">
          Contract
        </Typography>
        <Typography
          variant="bodyMedium"
          sx={(theme) => ({
            color: theme.palette.text.secondary,
            wordBreak: 'break-all',
          })}
        >
          {contractId}
        </Typography>
      </Box>
      <KpiStripSkeleton
        cells={[{ label: 'Total invocations' }, { label: 'Unique callers' }]}
      />
      <CardSkeleton />
    </Stack>
  );
}
