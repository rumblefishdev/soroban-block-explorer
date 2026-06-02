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
            <AssetAvatar leg={pool.asset_a} size={44} />
            <AssetAvatar leg={pool.asset_b} overlap size={44} />
          </Box>
        )}
        <Stack spacing={0.5}>
          <Stack direction="row" spacing={2}>
            <Typography variant="heading5SemiBold" component="h1">
              {pair}
            </Typography>
            {pool && <FeePill raw={pool.fee_percent} prefix />}
          </Stack>
          <IdentifierDisplay value={poolId} type="pool" linked={false} />
        </Stack>
      </Stack>
    </Box>
  );
}
