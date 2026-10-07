import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { SearchGroups, SearchHit } from '@rumblefish/api-types';

import { emptySearchState } from '../test-utils.js';
import { isMuxedSoranHit, type ExplorerSearchHit } from './searchHit.js';
import type { SoranMemo } from './soran.js';
import type { SoranLookup } from './useSoranLookup.js';
import { useSoranSearchResults } from './useSoranSearchResults.js';

const ACCOUNT = 'GBHKTFVBDUA6RYP5JM4SPZ76OXYAAHV4QHUOFV4S4TK342FMVGPHA2WN';
const OTHER_ACCOUNT =
  'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';
const CONTRACT = 'CCVKI6UYJDO34LO4D653IXCTPBGJOHJSJGSIOB4A46IH4ULNHS2MPQL7';
const MUXED =
  'MDHHA2WBSH4ZKIAWALPY4KVOC57ZUT6W6HWS3JBUQ4KFJRT6US4MWAAAAAAAAAAAFKV2W';
const NONE = { type: 'none' } as const;

function lookup(overrides: Partial<SoranLookup> = {}): SoranLookup {
  return {
    name: 'alice.nova',
    supported: true,
    armed: true,
    isFetching: false,
    isPaused: false,
    result: { kind: 'resolved', payment: { address: ACCOUNT, memo: NONE } },
    ask: vi.fn(),
    ...overrides,
  };
}

function indexed(q = 'alice.nova', groups?: SearchGroups) {
  return emptySearchState(q, { data: groups ? { groups } : undefined });
}

function indexedHit(
  entity_type: SearchHit['entity_type'],
  identifier: string
): SearchHit {
  return { entity_type, identifier, label: `Indexed ${identifier}` };
}

describe('useSoranSearchResults', () => {
  it.each([
    ['G', ACCOUNT, 'account', 'accounts'],
    ['C', CONTRACT, 'contract', 'contracts'],
    ['M', MUXED, 'account', 'accounts'],
  ] as const)(
    'places %s destinations in the native %s bucket',
    (_kind, address, entity, group) => {
      const soran = lookup({
        result: { kind: 'resolved', payment: { address, memo: NONE } },
      });
      const { result } = renderHook(() =>
        useSoranSearchResults('alice.nova', indexed(), soran)
      );
      const hit: ExplorerSearchHit = {
        entity_type: entity,
        identifier: address,
        label: 'alice.nova',
        soran: { name: 'alice.nova', memo: NONE },
      };
      expect(result.current.data?.groups[group]).toEqual([hit]);
      expect(result.current.counts[entity]).toBe(1);
      expect(result.current.totalCount).toBe(1);
      expect(result.current.activeTab).toBe(entity);
      expect(result.current.hitsForActiveTab).toEqual([hit]);
      expect(result.current.isFetching).toBe(false);
    }
  );

  it('prepends and deduplicates the exact destination without changing indexed data', () => {
    const other = indexedHit('account', OTHER_ACCOUNT);
    const duplicate = indexedHit('account', ACCOUNT);
    const asset = indexedHit('asset', 'NOVA');
    const source = indexed('alice.nova', {
      accounts: [other, duplicate, duplicate],
      assets: [asset],
    });
    const memo: SoranMemo = { type: 'id', value: '18446744073709551615' };
    const soran = lookup({
      result: { kind: 'resolved', payment: { address: ACCOUNT, memo } },
    });
    const { result } = renderHook(() =>
      useSoranSearchResults('alice.nova', source, soran)
    );
    expect(result.current.data?.groups.accounts).toEqual([
      {
        entity_type: 'account',
        identifier: ACCOUNT,
        label: 'alice.nova',
        soran: { name: 'alice.nova', memo },
      },
      other,
    ]);
    expect(result.current.data?.groups.assets).toEqual([asset]);
    expect(source.data?.groups.accounts).toEqual([other, duplicate, duplicate]);
    expect(result.current.counts.account).toBe(2);
    expect(result.current.counts.asset).toBe(1);
    expect(result.current.totalCount).toBe(3);
  });

  it('does not deduplicate a different entity bucket with the same identifier', () => {
    const asset = indexedHit('asset', ACCOUNT);
    const { result } = renderHook(() =>
      useSoranSearchResults(
        'alice.nova',
        indexed('alice.nova', { assets: [asset] }),
        lookup()
      )
    );
    expect(result.current.data?.groups.assets).toEqual([asset]);
    expect(result.current.totalCount).toBe(2);
  });

  it('immediately discards both old sources and resets the selected tab on a query edit', () => {
    const assets = [indexedHit('asset', 'NOVA')];
    const source = indexed('alice.nova', { assets });
    const { result, rerender } = renderHook(
      ({ q, soran }) => useSoranSearchResults(q, source, soran),
      {
        initialProps: { q: 'alice.nova', soran: lookup() },
      }
    );
    act(() => result.current.setActiveTab('asset'));
    expect(result.current.activeTab).toBe('asset');
    // Even a stale SoranLookup object cannot attach Alice's destination to Bob.
    rerender({ q: 'bob.nova', soran: lookup() });
    expect(result.current.data).toBeUndefined();
    expect(result.current.hitsForActiveTab).toEqual([]);
    expect(result.current.totalCount).toBe(0);
    expect(result.current.activeTab).toBe('transaction');
    expect(result.current.isFetching).toBe(true);
    rerender({ q: 'bob.nova', soran: lookup({ name: 'bob.nova' }) });
    expect(result.current.data?.groups.accounts?.[0].label).toBe('bob.nova');
    expect(result.current.data?.groups.assets).toBeUndefined();
    expect(result.current.activeTab).toBe('account');
  });

  it('uses trimmed query identity and canonical Soran labels', () => {
    const soran = lookup();
    const { result } = renderHook(() =>
      useSoranSearchResults(
        ' Alice.Nova ',
        indexed('Alice.Nova', { assets: [indexedHit('asset', 'NOVA')] }),
        soran
      )
    );
    expect(result.current.effectiveQuery).toBe('Alice.Nova');
    expect(result.current.totalCount).toBe(2);
    expect(result.current.data?.groups.accounts?.[0].label).toBe('alice.nova');
  });

  it('owns tab selection independently of indexed automatic tab updates', () => {
    const groups = {
      assets: [indexedHit('asset', 'NOVA')],
      contracts: [indexedHit('contract', CONTRACT)],
    };
    const source = indexed('alice.nova', groups);
    const { result, rerender } = renderHook(
      ({ state, soran }) => useSoranSearchResults('alice.nova', state, soran),
      {
        initialProps: {
          state: source,
          soran: lookup({ result: undefined, isFetching: true }),
        },
      }
    );
    expect(result.current.activeTab).toBe('contract');
    rerender({ state: { ...source, activeTab: 'asset' }, soran: lookup() });
    expect(result.current.activeTab).toBe('account');
    act(() => result.current.setActiveTab('asset'));
    rerender({ state: { ...source, activeTab: 'contract' }, soran: lookup() });
    expect(result.current.activeTab).toBe('asset');
    expect(result.current.hitsForActiveTab).toEqual(groups.assets);
  });

  it('does not reuse a previous selection after visiting an ordinary query', () => {
    const source = indexed('alice.nova', {
      assets: [indexedHit('asset', 'NOVA')],
    });
    const { result, rerender } = renderHook(
      ({ q }) => useSoranSearchResults(q, source, lookup()),
      { initialProps: { q: 'alice.nova' } }
    );
    act(() => result.current.setActiveTab('asset'));
    rerender({ q: 'NOVA' });
    expect(result.current).toBe(source);
    rerender({ q: 'alice.nova' });
    expect(result.current.activeTab).toBe('account');
  });

  it('preserves current indexed errors and retry while still showing a verified destination', () => {
    const error = new Error('Indexer unavailable');
    const refetch = vi.fn();
    const source = { ...indexed(), isError: true, error, refetch };
    const { result } = renderHook(() =>
      useSoranSearchResults('alice.nova', source, lookup())
    );
    expect(result.current.isError).toBe(true);
    expect(result.current.error).toBe(error);
    expect(result.current.refetch).toBe(refetch);
    expect(result.current.totalCount).toBe(1);
    expect(result.current.hitsForActiveTab[0]?.identifier).toBe(ACCOUNT);
  });

  it('does not relabel an old indexed error as an error for the new query', () => {
    const source = {
      ...indexed('old.nova'),
      isError: true,
      error: new Error('Old query failed'),
    };
    const { result } = renderHook(() =>
      useSoranSearchResults('alice.nova', source, lookup({ result: undefined }))
    );
    expect(result.current.isError).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it.each([
    { armed: false, isFetching: false, isPaused: false, result: undefined },
    { armed: true, isFetching: true, isPaused: false, result: undefined },
  ])(
    'treats waiting for a name as pending without inventing search data: %j',
    (pending) => {
      const { result } = renderHook(() =>
        useSoranSearchResults('alice.nova', indexed(), lookup(pending))
      );
      expect(result.current.isFetching).toBe(true);
      expect(result.current.data).toBeUndefined();
      expect(result.current.totalCount).toBe(0);
    }
  );

  it('does not mark a paused name as fetching or reuse a prior answer', () => {
    const soran = lookup({ isPaused: true });
    const { result } = renderHook(() =>
      useSoranSearchResults('alice.nova', indexed(), soran)
    );
    expect(result.current.isFetching).toBe(false);
    expect(result.current.data).toBeUndefined();
    expect(result.current.totalCount).toBe(0);
  });

  it('keeps real indexed hits when Soran fails and creates no artificial data', () => {
    const failure = lookup({
      result: { kind: 'failed', reason: 'Name expired' },
    });
    const source = indexed('alice.nova', {
      assets: [indexedHit('asset', 'NOVA')],
    });
    const { result, rerender } = renderHook(
      ({ state }) => useSoranSearchResults('alice.nova', state, failure),
      { initialProps: { state: source } }
    );
    expect(result.current.data).toBe(source.data);
    expect(result.current.totalCount).toBe(1);
    expect(result.current.activeTab).toBe('asset');
    expect(result.current.isFetching).toBe(false);
    rerender({ state: indexed() });
    expect(result.current.data).toBeUndefined();
    expect(result.current.totalCount).toBe(0);
  });

  it.each([
    ['mainnet', 'alice.nova', false],
    ['invalid name', 'XLM', true],
    ['federation', 'alice*example.org', true],
  ] as const)(
    'returns the original state untouched for %s',
    (_kind, q, supported) => {
      const source = indexed(q, { accounts: [indexedHit('account', ACCOUNT)] });
      const { result } = renderHook(() =>
        useSoranSearchResults(q, source, lookup({ supported }))
      );
      expect(result.current).toBe(source);
    }
  );
});

describe('isMuxedSoranHit', () => {
  it('identifies only an enriched M account row', () => {
    expect(
      isMuxedSoranHit({
        ...indexedHit('account', MUXED),
        soran: { name: 'mux.nova', memo: NONE },
      })
    ).toBe(true);
    expect(isMuxedSoranHit(indexedHit('account', MUXED))).toBe(false);
    expect(
      isMuxedSoranHit({
        ...indexedHit('account', ACCOUNT),
        soran: { name: 'alice.nova', memo: NONE },
      })
    ).toBe(false);
    expect(
      isMuxedSoranHit({
        ...indexedHit('contract', CONTRACT),
        soran: { name: 'alice.nova', memo: NONE },
      })
    ).toBe(false);
  });
});
