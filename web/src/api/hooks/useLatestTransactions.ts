import { listTransactionsOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { livePolicy, midpointPollDelay } from '../polling.js';

/**
 * Latest 10 transactions for the home page activity table. Polls
 * adaptively, aiming each fetch at the midpoint of the next ledger-close
 * gap (see `midpointPollDelay`), anchored on the newest row's `created_at`.
 * No cursor pagination — always the newest rows.
 */
export const useLatestTransactions = () =>
  useQuery({
    ...listTransactionsOptions({ query: { limit: 10 } }),
    ...livePolicy,
    refetchInterval: (query) =>
      midpointPollDelay(query.state.data?.data?.[0]?.created_at),
  });
