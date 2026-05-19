import { listInvocationsInfiniteOptions } from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

const PAGE_SIZE = 20;

/**
 * Fetches the paginated invocation appearance index for a contract
 * (`GET /contracts/:contract_id/invocations`). Cursor pagination; disabled
 * until an id is present.
 */
export const useContractInvocations = (contractId: string) =>
  useInfiniteQuery({
    ...listInvocationsInfiniteOptions({
      path: { contract_id: contractId },
      query: { limit: PAGE_SIZE },
    }),
    ...listPolicy,
    enabled: contractId.length > 0,
    initialPageParam: { path: { contract_id: contractId } },
    getNextPageParam: (lastPage) => lastPage.page.cursor ?? undefined,
  });
