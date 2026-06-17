import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { midpointPollDelay } from './polling.js';

// Pin "now" so delay = 1.5*5500 - (now - closedAt) is deterministic.
const NOW = Date.parse('2026-06-09T12:00:00.000Z');
const at = (msBeforeNow: number) => new Date(NOW - msBeforeNow).toISOString();

describe('midpointPollDelay', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('falls back to one cadence when the timestamp is missing or unparseable', () => {
    expect(midpointPollDelay(undefined)).toBe(5_500);
    expect(midpointPollDelay('not-a-date')).toBe(5_500);
  });

  it('aims at 1.5x cadence after close for a fresh block', () => {
    // elapsed 0 -> 8250, clamped to MAX 8250.
    expect(midpointPollDelay(at(0))).toBe(8_250);
  });

  it('shrinks the delay as time elapses since the close', () => {
    // elapsed 2000 -> 8250 - 2000 = 6250 (between floor and ceiling).
    expect(midpointPollDelay(at(2_000))).toBe(6_250);
  });

  it('clamps to the floor when the block is late (rechecks sooner)', () => {
    // elapsed 5000 -> 3250 -> clamp MIN 4000.
    expect(midpointPollDelay(at(5_000))).toBe(4_000);
    // elapsed 7000 -> 1250 -> clamp MIN 4000.
    expect(midpointPollDelay(at(7_000))).toBe(4_000);
    // very late -> still floor, never zero/negative.
    expect(midpointPollDelay(at(60_000))).toBe(4_000);
  });

  it('clamps to the ceiling for a future-dated close (clock skew)', () => {
    // elapsed negative -> > 8250 -> clamp MAX 8250.
    expect(midpointPollDelay(at(-3_000))).toBe(8_250);
  });
});
