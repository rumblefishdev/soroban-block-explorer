import { getPoolOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * Fetches a single liquidity-pool detail (`GET /liquidity-pools/:id`) —
 * asset pair, fee, reserves, total shares, participant_count. Disabled
 * until a pool id is present.
 */
export const usePoolDetail = (poolId: string) =>
  useQuery({
    ...getPoolOptions({ path: { pool_id: poolId } }),
    ...detailPolicy,
    enabled: poolId.length > 0,
  });
