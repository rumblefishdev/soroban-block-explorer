import { useNow } from '@rumblefish/soroban-block-explorer-ui';

import { useNetworkStats } from './useNetworkStats.js';

export type LiveStatus = 'live' | 'delayed' | 'offline';

/** ~4 ledger periods of slack before the feed is considered delayed. */
const LIVE_MAX_AGE_MS = 20_000;

/**
 * Single source of truth for network "liveness", consumed by both the
 * LiveIndicator pills and the footer status badge so they never disagree.
 * Derived from the newest indexed ledger's close time vs now, plus the
 * stats query error state.
 */
export function useLiveStatus(): LiveStatus {
  const { data, isError } = useNetworkStats();
  const now = useNow(1_000);

  if (isError) return 'offline';
  const closedAt = data?.latest_ledger_closed_at;
  const closedMs = closedAt ? new Date(closedAt).getTime() : NaN;
  if (Number.isFinite(closedMs) && now.getTime() - closedMs > LIVE_MAX_AGE_MS) {
    return 'delayed';
  }
  return 'live';
}
