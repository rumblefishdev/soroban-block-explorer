import { Box, Card, Divider, Skeleton, Stack } from '@mui/material';
import { alpha } from '@mui/material/styles';
import {
  TableSectionHeader,
  TableSkeleton,
} from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';
import { KpiCell } from '../detail/KpiCell.js';

import { HomeHero } from './HomeHero.js';
import { LiveIndicator } from './LiveIndicator.js';
import { ViewAllLink } from './ViewAllLink.js';

/**
 * Route-level Suspense fallback for the home page (`/`). Mirrors HomePage's
 * shape so the lazy-chunk fallback (phase A) matches the mounted page's own
 * loading state (phase B), killing the load-time layout flicker
 * (F-W6-LOADSKEL-1 / card 7.10).
 *
 * Hero is the REAL component (static, eager) so its block positions
 * pixel-match phase B (no vertical jump). The chain-overview + tables are
 * skeletons whose heights match the loaded layout — `KpiCell` loading and
 * loaded states are the same height, and `TableSkeleton` mirrors the real
 * tables. Kept query-free (no data hooks) so the fallback stays light.
 */
const KPI = [
  // Ledger label is the real LiveIndicator — matches Figma (node 4-2727) and
  // ChainOverview's own loading state (which also renders the live pip), so the
  // skeleton ledger cell looks the same as the mounted page.
  { label: <LiveIndicator />, caption: 'Current ledger' },
  { label: 'TPS', caption: 'Last 60s' },
  { label: 'Accounts', caption: 'Total' },
  { label: 'Contracts', caption: 'Soroban' },
];

function ChainOverviewSkeleton() {
  const cells = KPI.map((k) => (
    <KpiCell
      key={k.caption}
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
          borderRadius: `${theme.shape.radius.lg}px`,
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

function TableCardSkeleton({ title, to }: { title: string; to: string }) {
  return (
    <Card>
      {/* Same header as the loaded section (LiveIndicator badge + View All
          link) so the skeleton header isn't missing those vs the real card. */}
      <TableSectionHeader
        title={title}
        badge={<LiveIndicator />}
        action={<ViewAllLink to={to} />}
      />
      <TableSkeleton rows={10} columns={5} />
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
      <HomeHero />
      <Stack spacing={{ xs: 5, md: 10 }} sx={{ pb: 4 }}>
        <ChainOverviewSkeleton />
        <TableCardSkeleton
          title="Latest transactions"
          to={routes.transactions}
        />
        <TableCardSkeleton title="Latest Ledgers" to={routes.ledgers} />
      </Stack>
    </>
  );
}
