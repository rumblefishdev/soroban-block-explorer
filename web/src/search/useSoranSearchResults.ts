import type {
  EntityType,
  SearchGroups,
  SearchResults,
} from '@rumblefish/api-types';
import { useCallback, useEffect, useMemo, useState } from 'react';

import type { ExplorerSearchHit } from './searchHit.js';
import { parseSoranName } from './soran.js';
import { TAB_ORDER, type SearchResultsState } from './useSearchResults.js';
import type { SoranLookup } from './useSoranLookup.js';

const GROUP_FOR_TAB: Record<EntityType, keyof SearchGroups> = {
  transaction: 'transactions',
  account: 'accounts',
  contract: 'contracts',
  asset: 'assets',
  nft: 'nfts',
  pool: 'pools',
};

/** Put a verified name destination into the existing native result buckets. */
export function useSoranSearchResults(
  q: string,
  indexed: SearchResultsState,
  soran: SoranLookup
): SearchResultsState {
  const trimmed = q.trim();
  const name = parseSoranName(trimmed)?.name ?? null;
  const enabled = soran.supported && name != null;
  const queryKey = `${soran.supported ? 'testnet' : 'unsupported'}:${trimmed}`;
  const indexedCurrent = indexed.effectiveQuery === trimmed;
  const result =
    enabled &&
    soran.name === name &&
    soran.armed &&
    !soran.isFetching &&
    !soran.isPaused
      ? soran.result
      : undefined;
  const payment = result?.kind === 'resolved' ? result.payment : null;

  const soranHit = useMemo<ExplorerSearchHit | undefined>(() => {
    if (!payment || !name) return undefined;
    return {
      entity_type: payment.address.startsWith('C') ? 'contract' : 'account',
      identifier: payment.address,
      label: name,
      soran: { name, memo: payment.memo },
    };
  }, [payment, name]);

  const indexedData = indexedCurrent ? indexed.data : undefined;
  const data = useMemo<SearchResults | undefined>(() => {
    if (!soranHit) return indexedData;
    const key = GROUP_FOR_TAB[soranHit.entity_type];
    const groups = indexedData?.groups ?? {};
    return {
      ...indexedData,
      groups: {
        ...groups,
        [key]: [
          soranHit,
          ...(groups[key] ?? []).filter(
            (hit) => hit.identifier !== soranHit.identifier
          ),
        ],
      },
    };
  }, [indexedData, soranHit]);

  const counts = useMemo<Record<EntityType, number>>(
    () => ({
      transaction: data?.groups.transactions?.length ?? 0,
      account: data?.groups.accounts?.length ?? 0,
      contract: data?.groups.contracts?.length ?? 0,
      asset: data?.groups.assets?.length ?? 0,
      nft: data?.groups.nfts?.length ?? 0,
      pool: data?.groups.pools?.length ?? 0,
    }),
    [data]
  );
  const totalCount = TAB_ORDER.reduce((total, type) => total + counts[type], 0);

  // Only this hook owns the merged view's tab. The indexer's asynchronous
  // tab effect cannot switch away from a Soran hit or override a user's click.
  const [selection, setSelection] = useState<{
    key: string;
    tab: EntityType;
  } | null>(null);
  useEffect(() => setSelection(null), [queryKey]);
  const selectedTab = selection?.key === queryKey ? selection.tab : null;
  const defaultTab =
    soranHit?.entity_type ??
    TAB_ORDER.find((type) => counts[type] > 0) ??
    'transaction';
  const activeTab =
    selectedTab != null && counts[selectedTab] > 0 ? selectedTab : defaultTab;
  const setActiveTab = useCallback(
    (tab: EntityType) => setSelection({ key: queryKey, tab }),
    [queryKey]
  );

  // Hooks above run even for ordinary queries, whose existing state and
  // callbacks are returned by identity without changing their behavior.
  if (!enabled) return indexed;
  const waitingForName = !soran.isPaused && result === undefined;
  return {
    ...indexed,
    effectiveQuery: trimmed,
    data,
    isFetching: !indexedCurrent || indexed.isFetching || waitingForName,
    isError: indexedCurrent && indexed.isError,
    error: indexedCurrent ? indexed.error : null,
    counts,
    totalCount,
    activeTab,
    setActiveTab,
    hitsForActiveTab: data?.groups[GROUP_FOR_TAB[activeTab]] ?? [],
  };
}
