import { createContext, useContext, useEffect, useState } from 'react';

/**
 * App-wide refresh cadence for relative-time labels: 1s. Labels render
 * exact seconds ("12s ago") and update every second so the count advances
 * smoothly. A row fresher than the last tick is safe: `formatRelative`
 * clamps negative deltas to "just now".
 */
export const LIVE_TICK_MS = 1_000;

// One shared wall-clock ticker behind every relative-time label: a single
// interval drives one `now`, so all labels age in the same frame and the
// page holds one timer regardless of how many are mounted. The interval runs
// only while at least one label is subscribed.
let sharedNow = new Date();
const subscribers = new Set<(d: Date) => void>();
let handle: ReturnType<typeof setInterval> | null = null;

function subscribe(cb: (d: Date) => void): () => void {
  subscribers.add(cb);
  if (handle === null) {
    handle = setInterval(() => {
      sharedNow = new Date();
      subscribers.forEach((fn) => fn(sharedNow));
    }, LIVE_TICK_MS);
  }
  return () => {
    subscribers.delete(cb);
    if (subscribers.size === 0 && handle !== null) {
      clearInterval(handle);
      handle = null;
    }
  };
}

/**
 * Refetch-synced `now` override for live-polled tables. Populated by
 * `LiveNowProvider` (see `LiveNow.tsx`); `useNow` reads it transparently,
 * so consumers never branch on where `now` comes from.
 */
export const LiveNowContext = createContext<Date | null>(null);

export function useNow(): Date {
  const liveNow = useContext(LiveNowContext);
  const hasLive = liveNow !== null;
  const [now, setNow] = useState(sharedNow);
  useEffect(() => {
    // Inside a LiveNowProvider the provider drives `now` — skip the shared
    // ticker so the component doesn't re-render on a clock it never reads.
    if (hasLive) return undefined;
    return subscribe(setNow);
  }, [hasLive]);
  return liveNow ?? now;
}
