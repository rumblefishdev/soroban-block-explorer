import { Box, Stack, Typography } from '@mui/material';
import type { PoolItem } from '@rumblefish/api-types';
import { IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';
import { PageBreadcrumb } from '../detail/PageBreadcrumb.js';
import { AssetAvatar } from '../liquidity-pools/AssetAvatar.js';
import { FeePill } from '../liquidity-pools/FeePill.js';

import { assetLegLabel } from './helpers.js';

interface PoolDetailHeaderProps {
  poolId: string;
  pool?: PoolItem;
}

/**
 * Detail-page header for a liquidity pool. Renders the breadcrumb, a
 * stacked-avatar pair, the asset-pair name (`XLM / USDC`), a green
 * `Fee 0.30%` pill, and the truncated pool id below. Mirrors Figma
 * node `325:7098`.
 *
 * Stale state is no longer surfaced here — Figma intentionally does
 * not flag it on the header. Null reserves / TVL in the KPI strip and
 * Summary already communicate freshness; carrying a redundant pill in
 * the header was a pre-Figma carryover.
 *
 * When the pool data has not loaded yet (or 404 / error), falls back
 * to a minimal header so the page never renders blank.
 */
export function PoolDetailHeader({ poolId, pool }: PoolDetailHeaderProps) {
  const pair = pool
    ? `${assetLegLabel(pool.asset_a)} / ${assetLegLabel(pool.asset_b)}`
    : 'Liquidity pool';

  return (
    <Box>
      <PageBreadcrumb
        items={[
          { label: 'Liquidity Pools', to: routes.pools },
          { label: pair },
        ]}
      />
      <Stack direction="row" spacing={1.5} alignItems="center" sx={{ mb: 0.5 }}>
        {pool && (
          <Box sx={{ display: 'flex', alignItems: 'center' }}>
            <AssetAvatar leg={pool.asset_a} />
            <AssetAvatar leg={pool.asset_b} overlap />
          </Box>
        )}
        <Typography variant="heading3SemiBold" component="h1">
          {pair}
        </Typography>
        {pool && <FeePill raw={pool.fee_percent} prefix />}
      </Stack>
      {/* Static caption — we are already on this pool's detail page, so
          there is no link target that makes sense. Copy lives on the
          full-id row inside the Summary card. */}
      <IdentifierDisplay value={poolId} type="pool" linked={false} />
    </Box>
  );
}
