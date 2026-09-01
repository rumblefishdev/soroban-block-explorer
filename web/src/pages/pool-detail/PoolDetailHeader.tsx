import { Box, Chip, Stack, Typography } from '@mui/material';
import type { PoolItem } from '@rumblefish/api-types';
import { IdentifierDisplay } from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';
import { PageBreadcrumb } from '../detail/PageBreadcrumb.js';
import { PoolAssetPair } from '../pool-shared/PoolAssetPair.js';

import {
  isSorobanPool,
  poolLegViews,
  poolLegsLabel,
} from '../pool-shared/helpers.js';

interface PoolDetailHeaderProps {
  poolId: string;
  pool?: PoolItem;
}

export function PoolDetailHeader({ poolId, pool }: PoolDetailHeaderProps) {
  const legs = pool ? poolLegsLabel(pool) : 'Liquidity pool';

  return (
    <Box>
      <PageBreadcrumb
        items={[
          { label: 'Liquidity Pools', to: routes.pools },
          { label: legs },
        ]}
      />
      <Stack direction="row" spacing={1.5} alignItems="center" sx={{ mb: 0.5 }}>
        {pool && <PoolAssetPair legs={poolLegViews(pool)} size={44} />}
        <Stack spacing={0.5}>
          {/* Fee badge dropped (task 0348 F9): classic pools are all
              protocol-fixed at 0.30%, so the header pill was decorative.
              The fee stays as a quiet key-value in the Summary card. */}
          <Stack direction="row" spacing={1} alignItems="center">
            <Typography variant="heading5SemiBold" component="h1">
              {legs}
            </Typography>
            {/* Verified-operator protocol chip only (task 0374 T1). */}
            {pool?.protocol != null && (
              <Chip
                label={pool.protocol}
                size="small"
                variant="outlined"
                sx={{ textTransform: 'capitalize' }}
              />
            )}
          </Stack>
          <IdentifierDisplay
            value={poolId}
            type={pool != null && isSorobanPool(pool) ? 'contract' : 'pool'}
            linked={false}
          />
        </Stack>
      </Stack>
    </Box>
  );
}
