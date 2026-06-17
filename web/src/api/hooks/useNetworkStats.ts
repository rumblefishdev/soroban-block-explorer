import { getNetworkStatsOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { livePolicy, midpointPollDelay } from '../polling.js';

/**
 * Network stats (current ledger, TPS, totals) for the home KPIs, TopNav
 * counters and the LIVE badge. Polls on the same adaptive per-ledger
 * cadence as the activity feeds, anchored on `latest_ledger_closed_at`,
 * so the "current ledger" KPI steps +1 in lockstep with the tables
 * instead of jumping several sequences on a slower timer.
 */
export const useNetworkStats = () =>
  useQuery({
    ...getNetworkStatsOptions(),
    ...livePolicy,
    refetchInterval: (query) =>
      midpointPollDelay(query.state.data?.latest_ledger_closed_at ?? undefined),
  });
