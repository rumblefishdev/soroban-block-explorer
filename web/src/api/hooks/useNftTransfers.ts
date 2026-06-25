import { listNftTransfersOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

/**
 * `GET /v1/nfts/:contractId/:tokenId/transfers` — cursor-paginated
 * ownership history (mint / transfer / burn) for one NFT, keyed by
 * the composite `(contract_id, token_id)` external path.
 *
 * URL-as-state pagination.
 */
export const useNftTransfers = (
  contractId: string,
  tokenId: string,
  cursor: string | null = null,
  enabled = true
) =>
  useQuery({
    ...listNftTransfersOptions({
      path: { contract_id: contractId, token_id: tokenId },
      // Explicit page size (matches every other paginated list) so a full page
      // is 20 rows — keeps the loading skeleton (also 20 rows) the same height.
      query: { limit: 20, ...(cursor ? { cursor } : {}) },
    }),
    ...listPolicy,
    enabled: enabled && contractId !== '' && tokenId !== '',
  });
