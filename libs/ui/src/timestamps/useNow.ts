import { useEffect, useState } from 'react';

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
 * App-wide refresh cadence for relative-time labels: 10s. Labels render
 * exact seconds ("12s ago") but update in 10s steps — the deliberate
 * trade: calm pages over per-second churn in every row. Live-polled
 * tables refine this through `LiveNowProvider`, which overrides the tick
 * with a refetch-synced `now` (update per poll + the same 10s value as
 * the stall fallback). A row fresher than the last tick is safe:
 * `formatRelative` clamps negative deltas to "just now".
 */
export const LIVE_TICK_MS = 10_000;

/**
 * Refetch-synced `now` override for live-polled tables. Populated by
 * `LiveNowProvider` (see `LiveNow.tsx`); `useNow` reads it transparently,
 * so consumers never branch on where `now` comes from.
 */

export function useNow(intervalMs = LIVE_TICK_MS): Date {
  const safe =
    Number.isFinite(intervalMs) && intervalMs >= MIN_INTERVAL_MS
      ? intervalMs
      : MIN_INTERVAL_MS;
  const [now, setNow] = useState(() => tickers.get(safe)?.now ?? new Date());
  useEffect(() => subscribe(safe, setNow), [safe]);
  return now;
}
