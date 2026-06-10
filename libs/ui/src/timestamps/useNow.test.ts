import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LIVE_TICK_MS, useNow } from './useNow.js';

describe('useNow', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-06-10T12:00:00.000Z'));
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  // Regression guard: the app-wide cadence must stay fast enough to keep pace
  // with the live-polled feeds (~5s). If someone bumps this back to 30s, fresh
  // ledger/tx rows lag `now` and render "in the future" — this test fails first.
  it('LIVE_TICK_MS stays within live bounds (no slow-interval regression)', () => {
    expect(LIVE_TICK_MS).toBeGreaterThanOrEqual(500);
    expect(LIVE_TICK_MS).toBeLessThanOrEqual(5_000);
  });

  it('refreshes "now" at LIVE_TICK_MS by default (no per-component interval)', () => {
    const { result, unmount } = renderHook(() => useNow());
    const first = result.current.getTime();

    act(() => {
      vi.advanceTimersByTime(LIVE_TICK_MS + 10);
    });

    expect(result.current.getTime()).toBeGreaterThanOrEqual(
      first + LIVE_TICK_MS
    );
    unmount();
  });
});
