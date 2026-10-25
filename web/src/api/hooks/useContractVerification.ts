import { getVerificationOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * What independent source verifiers say about the contract's current WASM
 * (`GET /contracts/:contract_id/verification`, task 0639). Small and fetched
 * with the page: it decides whether the Verified Code tab and the header
 * chip exist. `unavailable` means no verifier service on this network and a
 * 404 no such contract or no WASM: both are answers, not failures; the
 * global policy does not retry a 4xx.
 */
export const useContractVerification = (contractId: string, enabled: boolean) =>
  useQuery({
    ...getVerificationOptions({ path: { contract_id: contractId } }),
    ...detailPolicy,
    enabled: enabled && contractId.length > 0,
    // A verification belongs to one contract: never show the previous
    // contract's answer while the next one loads.
    placeholderData: undefined,
  });
