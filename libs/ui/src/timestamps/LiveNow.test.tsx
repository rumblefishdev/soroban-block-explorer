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

  it('ticks every second between refetches', () => {
    const { result } = renderHook(() => useRefetchSyncedNow(T0));
    act(() => {
      vi.advanceTimersByTime(1_000);
    });
    // 1s cadence: labels advance each second even without a refetch.
    expect(result.current.getTime()).toBe(T0 + 1_000);
    act(() => {
      vi.advanceTimersByTime(8_000);
    });
    expect(result.current.getTime()).toBe(T0 + 9_000);
  });

  it('keeps aging when no refetch arrives (stalled feed)', () => {
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

  it('restarts the tick on every refetch', () => {
    const { result, rerender } = renderHook(
      ({ updatedAt }) => useRefetchSyncedNow(updatedAt),
      { initialProps: { updatedAt: T0 } }
    );
    // 9s in, a refetch lands — `now` jumps to the refetch moment and the
    // 1s timer restarts from here.
    act(() => {
      vi.advanceTimersByTime(9_000);
    });
    act(() => {
      rerender({ updatedAt: T0 + 9_000 });
    });
    expect(result.current.getTime()).toBe(T0 + 9_000);

    // 3s more: three 1s ticks since the refetch.
    act(() => {
      vi.advanceTimersByTime(3_000);
    });
    expect(result.current.getTime()).toBe(T0 + 12_000);
  });
});
