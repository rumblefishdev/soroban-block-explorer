import { useNow } from '@rumblefish/soroban-block-explorer-ui';

import { useNetworkStats } from './useNetworkStats.js';

export type LiveStatus = 'live' | 'delayed' | 'offline' | 'unknown';

/** ~4 ledger periods of slack before the feed is considered delayed. */
const LIVE_MAX_AGE_MS = 20_000;

/**
 * Single source of truth for network "liveness", consumed by the
 * LiveIndicator pill. Derived from the newest indexed ledger's close time
 * vs now, plus the stats query error state.
 *
 * States: `offline` on query error; `unknown` while loading or when the
 * close time is missing/unparseable (we must not claim `live` before the
 * data confirms it); `delayed` once the newest ledger is older than the
 * freshness window; `live` only with a finite, in-window timestamp.
 */
export function useLiveStatus(): LiveStatus {
  const { data, isLoading, isError } = useNetworkStats();
  const now = useNow();

  if (isError) return 'offline';
  if (isLoading || data == null) return 'unknown';

  const closedAt = data.latest_ledger_closed_at;
  const closedMs = closedAt ? new Date(closedAt).getTime() : NaN;
  if (!Number.isFinite(closedMs)) return 'unknown';
  if (now.getTime() - closedMs > LIVE_MAX_AGE_MS) return 'delayed';
  return 'live';
}
