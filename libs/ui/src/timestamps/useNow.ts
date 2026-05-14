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

export function useNow(intervalMs = 30_000): Date {
  const safe =
    Number.isFinite(intervalMs) && intervalMs >= MIN_INTERVAL_MS
      ? intervalMs
      : MIN_INTERVAL_MS;
  const [now, setNow] = useState(() => tickers.get(safe)?.now ?? new Date());
  useEffect(() => subscribe(safe, setNow), [safe]);
  return now;
}
