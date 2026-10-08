import { getAccountSponsorshipOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * Who pays each sponsored reserve of the account
 * (`GET /accounts/:account_id/sponsorship`), read live from RPC by the API.
 * Disabled until the caller knows the account has sponsored reserves.
 */
export const useAccountSponsorship = (accountId: string, enabled: boolean) =>
  useQuery({
    ...getAccountSponsorshipOptions({ path: { account_id: accountId } }),
    ...detailPolicy,
    enabled: enabled && accountId.length > 0,
  });
