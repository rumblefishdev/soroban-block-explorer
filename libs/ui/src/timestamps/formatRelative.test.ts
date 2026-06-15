import { describe, expect, it } from 'vitest';

import { formatRelative } from './formatRelative.js';

const NOW = new Date('2026-06-10T12:00:00.000Z');
/** `then` `ms` before NOW (negative ms = `then` in the future). */
const at = (ms: number) => new Date(NOW.getTime() - ms);

describe('formatRelative', () => {
  it('returns the fallback for invalid or non-positive timestamps', () => {
    expect(formatRelative(0, NOW)).toBe('—');
    expect(formatRelative('not-a-date', NOW)).toBe('—');
  });

  it('clamps future timestamps (client/chain clock skew) to "just now"', () => {
    // Recorded events can never be in the future — a negative delta is skew.
    expect(formatRelative(at(-12_000), NOW)).toBe('just now');
    expect(formatRelative(at(-1), NOW)).toBe('just now');
  });

  it('renders "just now" under 5 seconds', () => {
    expect(formatRelative(at(0), NOW)).toBe('just now');
    expect(formatRelative(at(4_999), NOW)).toBe('just now');
  });

  it('renders seconds, minutes, hours and days ago', () => {
    expect(formatRelative(at(12_000), NOW)).toBe('12s ago');
    expect(formatRelative(at(59_000), NOW)).toBe('59s ago');
    expect(formatRelative(at(90_000), NOW)).toBe('1 min ago');
    expect(formatRelative(at(2 * 3_600_000), NOW)).toBe('2 hours ago');
    expect(formatRelative(at(3_600_000), NOW)).toBe('1 hour ago');
    expect(formatRelative(at(25 * 3_600_000), NOW)).toBe('1 day ago');
    expect(formatRelative(at(3 * 86_400_000), NOW)).toBe('3 days ago');
  });

  it('accepts ISO strings and epoch millis', () => {
    expect(formatRelative(at(12_000).toISOString(), NOW)).toBe('12s ago');
    expect(formatRelative(at(12_000).getTime(), NOW)).toBe('12s ago');
  });
});
