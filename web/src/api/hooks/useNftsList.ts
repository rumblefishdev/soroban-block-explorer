import { listNftsOptions, type ListNftsData } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListNftsData['query']>;

/**
 * `GET /nfts` — cursor-paginated NFT list, optionally filtered by
 * collection, contract id, or name. URL-as-state pagination.
 */
export const useNftsList = (cursor: string | null = null, filters?: Filters) =>
  useQuery({
    ...listNftsOptions({
      query: { ...(filters ?? {}), ...(cursor ? { cursor } : {}) },
    }),
    ...listPolicy,
  });
