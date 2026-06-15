import { getNftOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * `GET /v1/nfts/:contractId/:tokenId` — a single NFT keyed by the
 * composite `(contract_id, token_id)` external path (ADR-aligned with
 * stellar.expert; no surrogate `nfts.id` leak in the URL).
 *
 * `metadata` is fetched at request time and is fail-soft: `null` means
 * the off-chain JSON could not be resolved, not that the NFT is missing.
 */
export const useNftDetail = (
  contractId: string,
  tokenId: string,
  enabled = true
) =>
  useQuery({
    ...getNftOptions({ path: { contract_id: contractId, token_id: tokenId } }),
    ...detailPolicy,
    enabled: enabled && contractId !== '' && tokenId !== '',
  });
