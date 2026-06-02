import { getInterfaceOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * Fetches the contract's public interface
 * (`GET /contracts/:contract_id/interface`) — declared function signatures.
 * `interface_metadata` is null for SAC / pre-upload contracts. Disabled
 * until an id is present.
 */
export const useContractInterface = (contractId: string) =>
  useQuery({
    ...getInterfaceOptions({ path: { contract_id: contractId } }),
    ...detailPolicy,
    enabled: contractId.length > 0,
  });
