import { describe, expect, it } from 'vitest';

import { LIVE_TICK_MS } from './useNow.js';

describe('useNow', () => {
  // Regression guard: the app-wide relative-time cadence must stay fast enough
  // to keep pace with the live-polled feeds (~5s). If someone bumps this back
  // to 30s, fresh ledger/tx rows lag `now` and render "in the future".
  it('LIVE_TICK_MS stays within live bounds (no slow-interval regression)', () => {
    expect(LIVE_TICK_MS).toBeGreaterThanOrEqual(500);
    expect(LIVE_TICK_MS).toBeLessThanOrEqual(5_000);
  });
});
