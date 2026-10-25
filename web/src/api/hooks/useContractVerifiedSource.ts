import { getVerifiedSourceOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * The verified source archive, unpacked
 * (`GET /contracts/:contract_id/verified-source`, task 0639). Fetched only
 * when the Verified Code tab opens. The archive is content-addressed, so a
 * fetched answer never goes stale for its hash; the tab compares that hash
 * with the current verification and refetches when the contract moved on.
 */
export const useContractVerifiedSource = (contractId: string) =>
  useQuery({
    ...getVerifiedSourceOptions({ path: { contract_id: contractId } }),
    ...detailPolicy,
    staleTime: Infinity,
    // A verification belongs to one contract: never show the previous
    // contract's answer while the next one loads.
    placeholderData: undefined,
    enabled: contractId.length > 0,
  });
