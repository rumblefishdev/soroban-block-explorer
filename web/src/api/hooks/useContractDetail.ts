import { getContractOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * Fetches the contract summary (`GET /contracts/:contract_id`) — deployer,
 * WASM hash, SAC flag, and windowed stats (`stats_window`). Disabled until
 * an id is present.
 */
export const useContractDetail = (contractId: string) =>
  useQuery({
    ...getContractOptions({ path: { contract_id: contractId } }),
    ...detailPolicy,
    enabled: contractId.length > 0,
  });
