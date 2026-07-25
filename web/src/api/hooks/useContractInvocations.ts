import { listInvocationsOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy, PAGE_SIZE } from '../polling.js';

/**
 * `GET /contracts/:contract_id/invocations` — cursor-paginated invocation
 * appearance index for a contract. Each cursor is a distinct queryKey, so
 * revisiting a cursor is a cache hit. URL-as-state pagination — caller
 * passes the current cursor from `useCursorPagination`.
 */
export const useContractInvocations = (
  contractId: string,
  cursor: string | null = null
) =>
  useQuery({
    ...listInvocationsOptions({
      path: { contract_id: contractId },
      query: { limit: PAGE_SIZE, ...(cursor ? { cursor } : {}) },
    }),
    ...listPolicy,
    enabled: contractId.length > 0,
  });
