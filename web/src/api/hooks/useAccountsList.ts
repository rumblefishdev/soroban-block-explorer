import {
  listAccountsOptions,
  type ListAccountsData,
} from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListAccountsData['query']>;

/**
 * `GET /v1/accounts` — cursor-paginated account list ordered by
 * `last_seen_ledger` (`order` flips asc/desc). Filters: `filter[q]`
 * (account_id prefix) and `filter[with_domain]`. URL-as-state pagination via
 * `useCursorPagination`.
 */
export const useAccountsList = (
  cursor: string | null = null,
  filters?: Filters
) => {
  const query: Record<string, unknown> = {
    ...(filters ?? {}),
    ...(cursor ? { cursor } : {}),
  };
  return useQuery({
    ...listAccountsOptions({ query: query as Filters }),
    ...listPolicy,
  });
};
