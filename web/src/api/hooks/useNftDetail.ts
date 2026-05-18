import { getNftOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * `GET /nfts/:id` — a single NFT keyed by the numeric `nfts.id` surrogate.
 * `metadata` is fetched at request time and is fail-soft: `null` means the
 * off-chain JSON could not be resolved, not that the NFT is missing.
 */
export const useNftDetail = (id: number, enabled = true) =>
  useQuery({
    ...getNftOptions({ path: { id } }),
    ...detailPolicy,
    enabled: enabled && Number.isInteger(id) && id > 0,
  });
