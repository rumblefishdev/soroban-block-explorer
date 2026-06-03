import { Box, Link, Skeleton, Stack, Typography } from '@mui/material';
import {
  formatInteger,
  isLedgerSequence,
} from '@rumblefish/soroban-block-explorer-ui';
import { Link as RouterLink, useParams } from 'react-router-dom';

import { routes } from '../../router/routes.js';
import { SummarySkeleton } from '../detail/SummarySkeleton.js';
import { TableSectionSkeleton } from '../detail/TableSectionSkeleton.js';

/**
 * Loading skeleton for the ledger detail page — header (Ledger / seq
 * breadcrumb + "Ledger {seq}" title) + summary card + transactions table,
 * matching the loaded layout. Used as BOTH route fallback (phase A) and the
 * page's `isLoading` return (phase B). Reads the sequence from the URL so the
 * title is real even in the fallback; the prev/next nav arrives with data.
 */
export function LedgerDetailSkeleton() {
  const { sequence: rawSequence } = useParams<{ sequence: string }>();
  const label =
    rawSequence != null && isLedgerSequence(rawSequence)
      ? formatInteger(Number(rawSequence))
      : rawSequence ?? '';
  return (
    <Stack spacing={3}>
      <Box>
        <Box sx={{ display: 'flex', gap: 0.5, mb: 1 }}>
          <Link
            component={RouterLink}
            to={routes.ledgers}
            variant="bodySmMedium"
            underline="hover"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            Ledger
          </Link>
          <Typography
            variant="bodySmMedium"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            /
          </Typography>
          <Typography
            variant="bodySmMedium"
            sx={(theme) => ({ color: theme.palette.text.primary })}
          >
            {label}
          </Typography>
        </Box>
        <Stack
          direction="row"
          justifyContent="space-between"
          alignItems="center"
          gap={2}
          sx={{ flexWrap: 'wrap' }}
        >
          <Typography variant="heading5SemiBold" component="h1">
            Ledger {label}
          </Typography>
          {/* LedgerNav (prev/next) placeholder */}
          <Stack direction="row" spacing={1}>
            <Skeleton variant="rounded" width={40} height={36} />
            <Skeleton variant="rounded" width={40} height={36} />
          </Stack>
        </Stack>
      </Box>
      <SummarySkeleton title="Summary" rows={7} />
      <TableSectionSkeleton
        title="Transactions in this ledger"
        rows={10}
        columns={5}
      />
    </Stack>
  );
}
