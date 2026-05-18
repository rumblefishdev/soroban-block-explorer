import { listNftTransfersInfiniteOptions } from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

/**
 * `GET /nfts/:id/transfers` — cursor-paginated ownership history (mint /
 * transfer / burn) for one NFT. The first page param carries the path id;
 * later pages thread only the cursor, the id persists via the query key.
 */
export const useNftTransfers = (id: number, enabled = true) =>
  useInfiniteQuery({
    ...listNftTransfersInfiniteOptions({ path: { id } }),
    ...listPolicy,
    enabled: enabled && Number.isInteger(id) && id > 0,
    initialPageParam: { path: { id } },
    getNextPageParam: (lastPage) => lastPage.page.cursor ?? undefined,
  });
