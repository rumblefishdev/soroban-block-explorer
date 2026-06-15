import { useEffect, useState, type PropsWithChildren } from 'react';

import { LIVE_TICK_MS, LiveNowContext } from './useNow.js';

/**
 * Refetch-synced "now" for live-polled tables.
 *
 * Inside a `LiveNowProvider`, `useNow` returns a `now` that updates **when
 * the data does** — every successful refetch bumps it, so the whole table
 * ages atomically (a new row lands and every "Xs ago" advances in the same
 * frame) instead of each row changing on an unrelated wall-clock tick.
 *
 * The fallback interval (same 1s as the global `useNow` tick) is the
 * safety net for a stalled feed (hidden tab, API outage): it keeps labels
 * aging honestly when no refetch arrives, and is reset by every refetch —
 * on a healthy per-ledger poll (~5.8s) it never fires.
 */
export function useRefetchSyncedNow(
  dataUpdatedAt: number,
  fallbackMs = LIVE_TICK_MS
): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    // Effect re-runs on every refetch (`dataUpdatedAt` change): update
    // `now` in the same commit as the fresh rows, and restart the
    // fallback timer so it only ever fires when the feed has stalled.
    setNow(new Date());
    const handle = setInterval(() => setNow(new Date()), fallbackMs);
    return () => clearInterval(handle);
  }, [dataUpdatedAt, fallbackMs]);
  return now;
}

interface LiveNowProviderProps {
  /** react-query `dataUpdatedAt` of the query feeding the wrapped table. */
  dataUpdatedAt: number;
}

export function LiveNowProvider({
  dataUpdatedAt,
  children,
}: PropsWithChildren<LiveNowProviderProps>) {
  const now = useRefetchSyncedNow(dataUpdatedAt);
  return (
    <LiveNowContext.Provider value={now}>{children}</LiveNowContext.Provider>
  );
}
