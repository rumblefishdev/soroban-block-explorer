import { Box, Skeleton, Stack, Typography } from '@mui/material';

import { routes } from '../../router/routes.js';
import { PageBreadcrumb } from '../detail/PageBreadcrumb.js';
import { SummarySkeleton } from '../detail/SummarySkeleton.js';
import { TableSectionSkeleton } from '../detail/TableSectionSkeleton.js';

/**
 * Loading skeleton for the asset detail page — header + the 2-col
 * [summary | metadata] layout + a transactions table placeholder, matching
 * the loaded shape. Used as BOTH the route Suspense fallback (phase A) and
 * the page's `isLoading` return (phase B). Query-free (skeleton primitives
 * only). The header shows the generic "Asset" title (the real code/icon
 * arrive with data, same as the page's own loading state).
 */
export function AssetDetailSkeleton() {
  return (
    <Stack spacing={3}>
      <Box>
        <PageBreadcrumb
          items={[{ label: 'Assets', to: routes.assets }, { label: 'Asset' }]}
        />
        {/* Mirror the loaded header (icon + code + name) so the first card
            doesn't jump down ~24px when data arrives. */}
        <Stack direction="row" spacing={1.5} alignItems="center">
          <Skeleton
            variant="circular"
            width={40}
            height={40}
            sx={{ flexShrink: 0 }}
          />
          <Box sx={{ minWidth: 0 }}>
            <Typography variant="heading5SemiBold" component="h1">
              Asset
            </Typography>
            <Skeleton variant="text" width={140} />
          </Box>
        </Stack>
      </Box>
      <Box
        sx={{
          display: 'flex',
          flexDirection: { xs: 'column', md: 'row' },
          gap: 3,
          alignItems: 'flex-start',
        }}
      >
        <Box sx={{ flex: 1, minWidth: 0, width: '100%' }}>
          <SummarySkeleton title="Summary" rows={5} />
        </Box>
        <Box sx={{ flex: 1, minWidth: 0, width: '100%' }}>
          <SummarySkeleton title="Metadata" meta="From TOML" rows={4} />
        </Box>
      </Box>
      <TableSectionSkeleton title="Latest transactions" rows={10} columns={5} />
    </Stack>
  );
}
