import { useQuery } from '@tanstack/react-query';
import { useEffect, useMemo, useState } from 'react';

import { getSearchOptions } from '@rumblefish/api-types';
import type {
  EntityType,
  SearchHit,
  SearchResponse,
} from '@rumblefish/api-types';

import { searchPolicy } from '../api/polling.js';
import { useDebounced } from './useDebounced.js';

export const TAB_ORDER: ReadonlyArray<EntityType> = [
  'transaction',
  'account',
  'contract',
  'asset',
  'nft',
  'pool',
];

export const ENTITY_LABEL: Record<EntityType, string> = {
  transaction: 'Transactions',
  account: 'Accounts',
  contract: 'Contract',
  asset: 'Token',
  nft: 'NFT',
  pool: 'Liquidity Pool',
};

interface UseSearchResultsParams {
  q: string;
  debounceMs?: number;

  treatRedirectAsResult?: boolean;
}

export interface SearchResultsState {
  effectiveQuery: string;
  data: SearchResponse | undefined;
  isFetching: boolean;
  isError: boolean;
  error: unknown;
  counts: Record<EntityType, number>;
  totalCount: number;
  activeTab: EntityType;
  setActiveTab: (t: EntityType) => void;
  hitsForActiveTab: readonly SearchHit[];
}

const GROUP_KEY: Record<
  EntityType,
  'transactions' | 'accounts' | 'contracts' | 'assets' | 'nfts' | 'pools'
> = {
  transaction: 'transactions',
  account: 'accounts',
  contract: 'contracts',
  asset: 'assets',
  nft: 'nfts',
  pool: 'pools',
};

export function useSearchResults({
  q,
  debounceMs = 300,
  treatRedirectAsResult = false,
}: UseSearchResultsParams): SearchResultsState {
  const debouncedRaw = useDebounced(q, debounceMs);
  const effectiveQuery = debouncedRaw.trim();
  const enabled = effectiveQuery.length > 0;

  const query = useQuery({
    ...getSearchOptions({ query: { q: effectiveQuery } }),
    ...searchPolicy,
    enabled,
  });

  const data = useMemo<SearchResponse | undefined>(() => {
    if (!query.data) return undefined;
    if (!treatRedirectAsResult || query.data.type !== 'redirect') {
      return query.data;
    }
    const hit: SearchHit = {
      entity_type: query.data.entity_type,
      identifier: query.data.entity_id,
      label: '',
      successful: query.data.successful ?? null,
      last_activity_at: query.data.last_activity_at ?? null,
    };
    return {
      type: 'results',
      groups: { [GROUP_KEY[query.data.entity_type]]: [hit] },
    };
  }, [query.data, treatRedirectAsResult]);

  const counts = useMemo<Record<EntityType, number>>(() => {
    const empty: Record<EntityType, number> = {
      transaction: 0,
      account: 0,
      contract: 0,
      asset: 0,
      nft: 0,
      pool: 0,
    };
    if (!data || data.type !== 'results') return empty;
    const { groups } = data;
    return {
      transaction: groups.transactions?.length ?? 0,
      account: groups.accounts?.length ?? 0,
      contract: groups.contracts?.length ?? 0,
      asset: groups.assets?.length ?? 0,
      nft: groups.nfts?.length ?? 0,
      pool: groups.pools?.length ?? 0,
    };
  }, [data]);

  const totalCount = TAB_ORDER.reduce((sum, t) => sum + counts[t], 0);

  const [activeTab, setActiveTab] = useState<EntityType>('transaction');

  useEffect(() => {
    if (!enabled || query.isFetching || totalCount === 0) return;
    if (counts[activeTab] > 0) return;
    const firstWithHits = TAB_ORDER.find((t) => counts[t] > 0);
    if (firstWithHits) setActiveTab(firstWithHits);
  }, [enabled, query.isFetching, totalCount, counts, activeTab]);

  const hitsForActiveTab = useMemo<readonly SearchHit[]>(() => {
    if (!data || data.type !== 'results') return [];
    const { groups } = data;
    switch (activeTab) {
      case 'transaction':
        return groups.transactions ?? [];
      case 'account':
        return groups.accounts ?? [];
      case 'contract':
        return groups.contracts ?? [];
      case 'asset':
        return groups.assets ?? [];
      case 'nft':
        return groups.nfts ?? [];
      case 'pool':
        return groups.pools ?? [];
    }
  }, [data, activeTab]);

  return {
    effectiveQuery,
    data,
    isFetching: enabled && query.isFetching,
    isError: query.isError,
    error: query.error,
    counts,
    totalCount,
    activeTab,
    setActiveTab,
    hitsForActiveTab,
  };
}
