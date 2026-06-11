import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useRefetchSyncedNow } from './LiveNow.js';

const T0 = Date.parse('2026-06-11T12:00:00.000Z');

describe('useRefetchSyncedNow', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(T0);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('bumps `now` when dataUpdatedAt changes (refetch-synced)', () => {
    const { result, rerender } = renderHook(
      ({ updatedAt }) => useRefetchSyncedNow(updatedAt),
      { initialProps: { updatedAt: T0 } }
    );
    expect(result.current.getTime()).toBe(T0);

    act(() => {
      vi.advanceTimersByTime(6_000);
    });
    act(() => {
      rerender({ updatedAt: T0 + 6_000 });
    });
    expect(result.current.getTime()).toBe(T0 + 6_000);
  });

  it('does not tick between refetches before the fallback window', () => {
    const { result } = renderHook(() => useRefetchSyncedNow(T0));
    act(() => {
      vi.advanceTimersByTime(9_000);
    });
    // Healthy-poll window (<10s): no wall-clock drift, labels stay put.
    expect(result.current.getTime()).toBe(T0);
  });

  it('falls back to aging when no refetch arrives (stalled feed)', () => {
    const { result } = renderHook(() => useRefetchSyncedNow(T0));
    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(result.current.getTime()).toBe(T0 + 10_000);
    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(result.current.getTime()).toBe(T0 + 20_000);
  });

  it('resets the fallback timer on every refetch', () => {
    const { result, rerender } = renderHook(
      ({ updatedAt }) => useRefetchSyncedNow(updatedAt),
      { initialProps: { updatedAt: T0 } }
    );
    // 9s in, a refetch lands — timer restarts from here.
    act(() => {
      vi.advanceTimersByTime(9_000);
    });
    act(() => {
      rerender({ updatedAt: T0 + 9_000 });
    });
    expect(result.current.getTime()).toBe(T0 + 9_000);

    // 9s more (18s since mount, 9s since refetch): fallback must NOT have
    // fired — a non-reset timer would have ticked at the 10s mark.
    act(() => {
      vi.advanceTimersByTime(9_000);
    });
    expect(result.current.getTime()).toBe(T0 + 9_000);
  });
});
