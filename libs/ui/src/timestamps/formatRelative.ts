const FALLBACK = '—';

export function formatRelative(
  then: Date | string | number,
  now: Date
): string {
  const thenMs =
    then instanceof Date
      ? then.getTime()
      : typeof then === 'string'
      ? new Date(then).getTime()
      : then;

  if (!Number.isFinite(thenMs) || thenMs <= 0) return FALLBACK;

  // Every caller renders a recorded on-chain event (ledger close, tx created) —
  // always in the past. A negative delta is client/chain clock skew OR a row
  // fresher than the last 10s `useNow` tick, so clamp to 0 ("just now")
  // instead of rendering "in 12s".
  const sec = Math.max(0, Math.floor((now.getTime() - thenMs) / 1000));
  if (sec < 5) return 'just now';
  if (sec < 60) return `${sec}s ago`;

  const min = Math.floor(sec / 60);
  if (min < 60) return `${min} min ago`;

  const hour = Math.floor(min / 60);
  if (hour < 24) return `${hour} hour${hour === 1 ? '' : 's'} ago`;

  const day = Math.floor(hour / 24);
  return `${day} day${day === 1 ? '' : 's'} ago`;
}
