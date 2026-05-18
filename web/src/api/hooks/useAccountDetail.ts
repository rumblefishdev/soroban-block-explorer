import { getAccountOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * Fetches the account summary (`GET /accounts/:account_id`) — balances,
 * sequence number, first/last seen ledger. Disabled until an id is present.
 */
export const useAccountDetail = (accountId: string) =>
  useQuery({
    ...getAccountOptions({ path: { account_id: accountId } }),
    ...detailPolicy,
    enabled: accountId.length > 0,
  });
