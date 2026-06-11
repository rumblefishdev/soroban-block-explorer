import { describe, expect, it } from 'vitest';

import { LIVE_TICK_MS } from './useNow.js';

describe('useNow', () => {
  // Regression guard: the wall-clock tick and the LiveNowProvider stall
  // fallback share this value by design — relative-time labels update in
  // 10s steps everywhere, refetch-synced tables just add per-poll bumps.
  // `formatRelative` clamps negative deltas, so a fresh row landing
  // mid-window renders "just now", never "in Xs".
  it('LIVE_TICK_MS is the shared 10s relative-time cadence', () => {
    expect(LIVE_TICK_MS).toBe(10_000);
  });
});
