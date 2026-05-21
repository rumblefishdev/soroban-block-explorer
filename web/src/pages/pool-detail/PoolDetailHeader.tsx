import { Box, Stack, Typography } from '@mui/material';
import type { PoolItem } from '@rumblefish/api-types';
import { Chip, IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';
import { poolIdHexToStrkey } from '../../utils/poolIdStrkey.js';
import { PageBreadcrumb } from '../detail/PageBreadcrumb.js';

import { assetLegLabel, isPoolStale } from './helpers.js';

interface PoolDetailHeaderProps {
  poolId: string;
  pool?: PoolItem;
}

/**
 * Detail-page header for a liquidity pool. Renders the breadcrumb, the
 * asset-pair name (`XLM / USDC`), an Active / Stale status badge, and the
 * truncated pool id below. Mirrors the Figma node `325:7098` header band.
 *
 * When the pool data has not loaded yet (or 404 / error), falls back to a
 * minimal header so the page never renders blank.
 */
export function PoolDetailHeader({ poolId, pool }: PoolDetailHeaderProps) {
  const pair = pool
    ? `${assetLegLabel(pool.asset_a)} / ${assetLegLabel(pool.asset_b)}`
    : 'Liquidity pool';
  const stale = pool ? isPoolStale(pool.latest_snapshot_at) : false;
  // Backend serves the pool id as 64-char hex; render the SEP-23 `L...`
  // strkey (the form Stellar Laboratory / stellar.expert show) but keep
  // the route href on hex so backend lookups still resolve.
  const strkey = poolIdHexToStrkey(poolId);

  // Per-leg asset icons (Figma 325:7098) are deferred until the backend
  // surfaces `icon_url` on `PoolAssetLeg`; a letter-avatar fallback is
  // not the intent.
  return (
    <Box>
      <PageBreadcrumb
        items={[
          { label: 'Liquidity Pools', to: routes.pools },
          { label: pair },
        ]}
      />
      <Stack direction="row" spacing={1.5} alignItems="center" sx={{ mb: 0.5 }}>
        <Typography variant="heading3SemiBold" component="h1">
          {pair}
        </Typography>
        {pool && (
          <Chip
            size="md"
            color={stale ? 'neutral' : 'accent'}
            label={stale ? 'Stale' : 'Active'}
          />
        )}
      </Stack>
      <IdentifierDisplay value={strkey} type="pool" />
    </Box>
  );
}
