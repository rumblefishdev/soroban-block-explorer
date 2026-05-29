import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { usePageHandlers } from './usePageHandlers.js';

describe('usePageHandlers', () => {
  it('disables both directions when the page has no cursors', () => {
    const goNext = vi.fn();
    const goPrev = vi.fn();
    const { result } = renderHook(() =>
      usePageHandlers({ next_cursor: null, prev_cursor: null }, goNext, goPrev)
    );
    expect(result.current.canNext).toBe(false);
    expect(result.current.canPrev).toBe(false);
  });

  it('disables both directions when the page itself is undefined', () => {
    const { result } = renderHook(() =>
      usePageHandlers(undefined, vi.fn(), vi.fn())
    );
    expect(result.current.canNext).toBe(false);
    expect(result.current.canPrev).toBe(false);
  });

  it('enables Next and forwards the cursor on handleNext', () => {
    const goNext = vi.fn();
    const { result } = renderHook(() =>
      usePageHandlers({ next_cursor: 'c1', prev_cursor: null }, goNext, vi.fn())
    );
    expect(result.current.canNext).toBe(true);
    expect(result.current.canPrev).toBe(false);
    act(() => result.current.handleNext());
    expect(goNext).toHaveBeenCalledWith('c1');
  });

  it('enables Prev and forwards the cursor on handlePrev', () => {
    const goPrev = vi.fn();
    const { result } = renderHook(() =>
      usePageHandlers({ next_cursor: null, prev_cursor: 'c0' }, vi.fn(), goPrev)
    );
    expect(result.current.canPrev).toBe(true);
    act(() => result.current.handlePrev());
    expect(goPrev).toHaveBeenCalledWith('c0');
  });

  it('no-ops handleNext / handlePrev when the matching cursor is absent', () => {
    const goNext = vi.fn();
    const goPrev = vi.fn();
    const { result } = renderHook(() =>
      usePageHandlers({ next_cursor: null, prev_cursor: null }, goNext, goPrev)
    );
    act(() => {
      result.current.handleNext();
      result.current.handlePrev();
    });
    expect(goNext).not.toHaveBeenCalled();
    expect(goPrev).not.toHaveBeenCalled();
  });
});
