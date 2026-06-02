import { Box, Card, Link, Skeleton, Stack, Typography } from '@mui/material';
import {
  CardSkeleton,
  TableSectionHeader,
  TableSkeleton,
} from '@rumblefish/soroban-block-explorer-ui';
import { Link as RouterLink, useParams } from 'react-router-dom';

import { routes } from '../../router/routes.js';

/**
 * Loading skeleton for the NFT detail page — breadcrumb + the 2-col
 * [media square | details] layout (title + summary card + traits card) +
 * transfers table, matching the loaded shape. Used as BOTH route fallback
 * (phase A) and the page's `isLoading` return (phase B). Reads the token id
 * from the URL for the breadcrumb; collection name arrives with data.
 */
export function NftDetailSkeleton() {
  const { tokenId = '' } = useParams<{ contractId: string; tokenId: string }>();
  return (
    <Stack spacing={3}>
      <Box>
        <Box sx={{ display: 'flex', gap: 0.5, flexWrap: 'wrap', mb: 1 }}>
          <Link
            component={RouterLink}
            to={routes.nfts}
            variant="bodySmMedium"
            underline="hover"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            NFTs
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
            #{tokenId}
          </Typography>
        </Box>
      </Box>

      <Box
        sx={{
          display: 'flex',
          gap: 3,
          alignItems: 'flex-start',
          flexWrap: 'wrap',
        }}
      >
        <Skeleton
          variant="rounded"
          width={308}
          height={308}
          sx={{ borderRadius: '12px', flexShrink: 0 }}
        />
        <Stack spacing={2} sx={{ flex: 1, minWidth: 320 }}>
          <Skeleton variant="text" width={240} height={32} />
          <CardSkeleton />
          <CardSkeleton />
        </Stack>
      </Box>

      <Card>
        <TableSectionHeader title="Transfer history" />
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
    </Stack>
  );
}
