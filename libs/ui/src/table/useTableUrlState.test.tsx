import { act, renderHook } from '@testing-library/react';
import { type ReactNode } from 'react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { describe, expect, it } from 'vitest';

import { useTableUrlState } from './useTableUrlState.js';

function wrap(initialEntries: string[] = ['/']) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <MemoryRouter initialEntries={initialEntries}>{children}</MemoryRouter>
    );
  };
}

describe('useTableUrlState', () => {
  it('seeds state from URL params (cursor, sort, dir, filters)', () => {
    const { result } = renderHook(
      () => useTableUrlState({ filterKeys: ['q', 'op'] }),
      {
        wrapper: wrap(['/?cursor=c0&sort=fee&dir=asc&q=GBQF&op=PAYMENT']),
      }
    );
    expect(result.current.state.cursor).toBe('c0');
    expect(result.current.state.sortBy).toBe('fee');
    expect(result.current.state.sortDir).toBe('asc');
    expect(result.current.state.filters).toEqual({ q: 'GBQF', op: 'PAYMENT' });
  });

  it('uses defaults when params are absent', () => {
    const { result } = renderHook(
      () =>
        useTableUrlState({ defaultSortBy: 'ledger', defaultSortDir: 'desc' }),
      { wrapper: wrap() }
    );
    expect(result.current.state.cursor).toBeNull();
    expect(result.current.state.sortBy).toBe('ledger');
    expect(result.current.state.sortDir).toBe('desc');
  });

  it('setCursor writes/clears the URL', () => {
    const probe = { search: '' };
    function Probe() {
      const { search } = useLocation();
      probe.search = search;
      return null;
    }
    const { result } = renderHook(() => useTableUrlState(), {
      wrapper: ({ children }) => (
        <MemoryRouter initialEntries={['/']}>
          {children}
          <Probe />
        </MemoryRouter>
      ),
    });

    act(() => result.current.setCursor('c1'));
    expect(probe.search).toContain('cursor=c1');

    act(() => result.current.setCursor(null));
    expect(probe.search).not.toContain('cursor=');
  });

  it('setSort drops the cursor param', () => {
    const probe = { search: '' };
    function Probe() {
      const { search } = useLocation();
      probe.search = search;
      return null;
    }
    const { result } = renderHook(() => useTableUrlState(), {
      wrapper: ({ children }) => (
        <MemoryRouter initialEntries={['/?cursor=c0']}>
          {children}
          <Probe />
        </MemoryRouter>
      ),
    });

    act(() => result.current.setSort('fee', 'asc'));
    expect(probe.search).toContain('sort=fee');
    expect(probe.search).toContain('dir=asc');
    expect(probe.search).not.toContain('cursor=');
  });

  it('setFilter drops the cursor param', () => {
    const probe = { search: '' };
    function Probe() {
      const { search } = useLocation();
      probe.search = search;
      return null;
    }
    const { result } = renderHook(
      () => useTableUrlState({ filterKeys: ['q'] }),
      {
        wrapper: ({ children }) => (
          <MemoryRouter initialEntries={['/?cursor=c0']}>
            {children}
            <Probe />
          </MemoryRouter>
        ),
      }
    );

    act(() => result.current.setFilter('q', 'GBQF'));
    expect(probe.search).toContain('q=GBQF');
    expect(probe.search).not.toContain('cursor=');
  });

  it('clearFilters drops ALL filter keys + cursor in one update', () => {
    // Regression: doing this via two sequential setFilter(key, null) calls
    // fails — react-router hands each synchronous functional update the
    // same pre-render params, so the second clobbers the first and only
    // one key clears. clearFilters must drop every key at once.
    const probe = { search: '' };
    function Probe() {
      const { search } = useLocation();
      probe.search = search;
      return null;
    }
    const { result } = renderHook(
      () => useTableUrlState({ filterKeys: ['q', 'type'] }),
      {
        wrapper: ({ children }) => (
          <MemoryRouter initialEntries={['/?q=ABC&type=token&cursor=c0']}>
            {children}
            <Probe />
          </MemoryRouter>
        ),
      }
    );

    act(() => result.current.clearFilters());
    expect(probe.search).not.toContain('q=');
    expect(probe.search).not.toContain('type=');
    expect(probe.search).not.toContain('cursor=');
  });

  it('clearFilters leaves sort/dir alone (they are not filter keys)', () => {
    // Sort lives in the dedicated `sort`/`dir` params, never in filterKeys,
    // so "Clear filters" narrows nothing about the ordering.
    const probe = { search: '' };
    function Probe() {
      const { search } = useLocation();
      probe.search = search;
      return null;
    }
    const { result } = renderHook(
      () => useTableUrlState({ filterKeys: ['q', 'domain'] }),
      {
        wrapper: ({ children }) => (
          <MemoryRouter initialEntries={['/?q=ABC&domain=1&sort=xlm_desc&dir=asc']}>
            {children}
            <Probe />
          </MemoryRouter>
        ),
      }
    );

    act(() => result.current.clearFilters());
    expect(probe.search).not.toContain('q=');
    expect(probe.search).not.toContain('domain=');
    expect(probe.search).toContain('sort=xlm_desc');
    expect(probe.search).toContain('dir=asc');
  });

  it('cursorParam option lets multiple tables coexist', () => {
    const probe = { search: '' };
    function Probe() {
      const { search } = useLocation();
      probe.search = search;
      return null;
    }
    const { result } = renderHook(
      () => useTableUrlState({ cursorParam: 'cursor_p' }),
      {
        wrapper: ({ children }) => (
          <MemoryRouter initialEntries={['/']}>
            {children}
            <Probe />
          </MemoryRouter>
        ),
      }
    );
    act(() => result.current.setCursor('cp'));
    expect(probe.search).toContain('cursor_p=cp');
    expect(probe.search).not.toContain('cursor=');
  });
});
