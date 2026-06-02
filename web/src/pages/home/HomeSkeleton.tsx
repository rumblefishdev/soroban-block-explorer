import { Box, Card, Divider, Skeleton, Stack } from '@mui/material';
import { alpha } from '@mui/material/styles';
import {
  TableSectionHeader,
  TableSkeleton,
} from '@rumblefish/soroban-block-explorer-ui';

import { KpiCell } from '../detail/KpiCell.js';

/**
 * Route-level Suspense fallback for the home page (`/`). Mirrors HomePage's
 * shape — hero, the four-stat chain overview, and the two latest-records
 * tables — so the lazy-chunk fallback (phase 1) matches the mounted page's
 * own loading state (phase 2), killing the load-time layout flicker
 * (F-W6-LOADSKEL-1 / card 7.10). Intentionally self-contained: it reuses
 * only eagerly-bundled primitives (no data hooks, no lazy sections) so it
 * stays out of the home chunk.
 */
const KPI = [
  { label: 'Ledger', caption: 'Current ledger' },
  { label: 'TPS', caption: 'Last 60s' },
  { label: 'Accounts', caption: 'Total' },
  { label: 'Contracts', caption: 'Soroban' },
];

function ChainOverviewSkeleton() {
  const cells = KPI.map((k) => (
    <KpiCell
      key={k.label}
      card={false}
      align="center"
      valueVariant="heading4SemiBold"
      labelVariant="bodyMedium"
      label={k.label}
      caption={k.caption}
      loading
    />
  ));
  return (
    <Box sx={{ display: 'flex', justifyContent: 'center' }}>
      <Box
        sx={(theme) => ({
          width: '100%',
          maxWidth: 1064,
          borderRadius: '16px',
          border: `1px solid ${theme.palette.stroke.default}`,
          backgroundColor: alpha(theme.palette.surface.grayMainAlt, 0.8),
          backdropFilter: 'blur(6px)',
          overflow: 'hidden',
        })}
      >
        <Box
          sx={(theme) => ({
            display: { xs: 'grid', md: 'none' },
            gridTemplateColumns: '1fr 1fr',
            gap: '1px',
            backgroundColor: theme.palette.stroke.default,
            '& > *': { backgroundColor: theme.palette.surface.grayMainAlt },
          })}
        >
          {cells}
        </Box>
        <Stack
          direction="row"
          alignItems="stretch"
          divider={<Divider orientation="vertical" flexItem />}
          sx={{ width: '100%', display: { xs: 'none', md: 'flex' } }}
        >
          {cells}
        </Stack>
      </Box>
    </Box>
  );
}

function TableCardSkeleton({ title }: { title: string }) {
  return (
    <Card>
      <TableSectionHeader title={title} />
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={10} columns={5} />
      </Box>
      <Box
        sx={{
          px: 2,
          py: 1.5,
          borderTop: (theme) => `1px solid ${theme.palette.stroke.default}`,
          backgroundColor: (theme) => theme.palette.surface.grayMainAlt,
        }}
      >
        <Skeleton variant="text" width={120} />
      </Box>
    </Card>
  );
}

export function HomeSkeleton() {
  return (
    <>
      <Box sx={{ pt: { xs: 4, md: 8 }, pb: { xs: 3, md: 5 } }}>
        <Stack spacing={4} alignItems="center">
          <Stack spacing={1.5} alignItems="center" sx={{ width: '100%' }}>
            <Skeleton variant="text" width={280} height={48} />
            <Skeleton variant="text" width={220} height={48} />
          </Stack>
          <Skeleton
            variant="rounded"
            height={56}
            sx={{ width: '100%', maxWidth: 640, borderRadius: '12px' }}
          />
        </Stack>
      </Box>
      <Stack spacing={{ xs: 5, md: 10 }} sx={{ pb: 4 }}>
        <ChainOverviewSkeleton />
        <TableCardSkeleton title="Latest transactions" />
        <TableCardSkeleton title="Latest Ledgers" />
      </Stack>
    </>
  );
}
