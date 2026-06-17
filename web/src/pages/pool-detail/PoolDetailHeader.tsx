import { Box, Stack, Typography } from '@mui/material';
import type { PoolItem } from '@rumblefish/api-types';
import { IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';
import { PageBreadcrumb } from '../detail/PageBreadcrumb.js';
import { FeePill } from '../pool-shared/FeePill.js';
import { PoolAssetPair } from '../pool-shared/PoolAssetPair.js';

import { assetLegLabel } from '../pool-shared/helpers.js';

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
        {pool && <PoolAssetPair a={pool.asset_a} b={pool.asset_b} size={44} />}
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
