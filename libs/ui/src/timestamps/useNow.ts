import { createContext, useContext, useEffect, useState } from 'react';

const tickers = new Map<
  number,
  {
    now: Date;
    subs: Set<(d: Date) => void>;
    handle: ReturnType<typeof setInterval>;
  }
>();

function subscribe(intervalMs: number, cb: (d: Date) => void): () => void {
  let t = tickers.get(intervalMs);
  if (!t) {
    const entry = {
      now: new Date(),
      subs: new Set<(d: Date) => void>(),
      handle: undefined as unknown as ReturnType<typeof setInterval>,
    };
    entry.handle = setInterval(() => {
      entry.now = new Date();
      entry.subs.forEach((fn) => fn(entry.now));
    }, intervalMs);
    tickers.set(intervalMs, entry);
    t = entry;
  }
  t.subs.add(cb);
  return () => {
    const entry = tickers.get(intervalMs);
    if (!entry) return;
    entry.subs.delete(cb);
    if (entry.subs.size === 0) {
      clearInterval(entry.handle);
      tickers.delete(intervalMs);
    }
  };
}

const MIN_INTERVAL_MS = 500;

/**
 * App-wide refresh cadence for relative-time labels: 1s. Labels render
 * exact seconds ("12s ago") and update every second so the count advances
 * smoothly. Live-polled tables refine this through `LiveNowProvider`, which
 * overrides the tick with a refetch-synced `now` (update per poll + the
 * same 1s value as the stall fallback). A row fresher than the last tick is
 * safe: `formatRelative` clamps negative deltas to "just now".
 */
export const LIVE_TICK_MS = 1_000;

/**
 * Refetch-synced `now` override for live-polled tables. Populated by
 * `LiveNowProvider` (see `LiveNow.tsx`); `useNow` reads it transparently,
 * so consumers never branch on where `now` comes from.
 */
export const LiveNowContext = createContext<Date | null>(null);

export function useNow(intervalMs = LIVE_TICK_MS): Date {
  const liveNow = useContext(LiveNowContext);
  const hasLive = liveNow !== null;
  const safe =
    Number.isFinite(intervalMs) && intervalMs >= MIN_INTERVAL_MS
      ? intervalMs
      : MIN_INTERVAL_MS;
  const [now, setNow] = useState(() => tickers.get(safe)?.now ?? new Date());
  useEffect(() => {
    // Inside a LiveNowProvider the provider drives `now` — skip the
    // ticker subscription so the component doesn't re-render on a
    // wall-clock it never reads.
    if (hasLive) return undefined;
    return subscribe(safe, setNow);
  }, [safe, hasLive]);
  return liveNow ?? now;
}
