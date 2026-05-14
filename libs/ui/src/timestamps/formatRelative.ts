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
  const deltaMs = now.getTime() - thenMs;
  const absMs = Math.abs(deltaMs);
  const future = deltaMs < 0;

  const sec = Math.round(absMs / 1000);
  if (sec < 5) return future ? 'in a moment' : 'just now';
  if (sec < 60) return future ? `in ${sec}s` : `${sec}s ago`;

  const min = Math.round(sec / 60);
  if (min < 60) return future ? `in ${min} min` : `${min} min ago`;

  const hour = Math.round(min / 60);
  if (hour < 24)
    return future
      ? `in ${hour} hour${hour === 1 ? '' : 's'}`
      : `${hour} hour${hour === 1 ? '' : 's'} ago`;

  const day = Math.round(hour / 24);
  return future
    ? `in ${day} day${day === 1 ? '' : 's'}`
    : `${day} day${day === 1 ? '' : 's'} ago`;
}
