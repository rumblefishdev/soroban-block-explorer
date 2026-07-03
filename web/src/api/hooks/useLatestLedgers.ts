import { listLedgersOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { livePolicy, midpointPollDelay } from '../polling.js';

/**
 * Latest 10 ledgers for the home page activity table. Polls adaptively,
 * aiming each fetch at the midpoint of the next ledger-close gap (see
 * `midpointPollDelay`), anchored on the newest row's `closed_at`. No cursor
 * pagination — always the newest rows.
 */
export const useLatestLedgers = () =>
  useQuery({
    ...listLedgersOptions({ query: { limit: 10 } }),
    ...livePolicy,
    refetchInterval: (query) =>
      midpointPollDelay(query.state.data?.data?.[0]?.closed_at),
  });
