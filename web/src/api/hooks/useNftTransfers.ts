import { listNftTransfersOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

/**
 * `GET /nfts/:id/transfers` — cursor-paginated ownership history (mint /
 * transfer / burn) for one NFT. URL-as-state pagination.
 */
export const useNftTransfers = (
  id: number,
  cursor: string | null = null,
  enabled = true
) =>
  useQuery({
    ...listNftTransfersOptions({
      path: { id },
      query: cursor ? { cursor } : undefined,
    }),
    ...listPolicy,
    enabled: enabled && Number.isInteger(id) && id > 0,
  });
