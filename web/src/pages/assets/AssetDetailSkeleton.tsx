import { Box, Card, Stack, Typography } from '@mui/material';
import {
  CardSkeleton,
  TableSkeleton,
} from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';
import { PageBreadcrumb } from '../detail/PageBreadcrumb.js';

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
        <Typography variant="heading5SemiBold" component="h1">
          Asset
        </Typography>
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
          <CardSkeleton />
        </Box>
        <Box sx={{ flex: 1, minWidth: 0, width: '100%' }}>
          <CardSkeleton />
        </Box>
      </Box>
      <Card>
        <TableSkeleton rows={10} columns={5} />
      </Card>
    </Stack>
  );
}
