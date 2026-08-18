import type { QueryClient } from '@tanstack/react-query';

const SDK_IDS_BY_RESOURCE = {
  transactions: new Set([
    'listTransactions',
    'getTransaction',
    'listAccountTransactions',
    'listAssetTransactions',
    'listPoolActivity',
  ]),
  accounts: new Set(['getAccount', 'listAccountTransactions']),
  ledgers: new Set(['listLedgers', 'getLedger']),
  assets: new Set(['listAssets', 'getAsset', 'listAssetTransactions']),
  contracts: new Set([
    'getContract',
    'getInterface',
    'listInvocations',
    'listEvents',
  ]),
  nfts: new Set(['listNfts', 'getNft', 'listNftTransfers']),
  pools: new Set([
    'listPools',
    'getPool',
    'getPoolChart',
    'listParticipants',
    'listPoolActivity',
  ]),
  search: new Set(['getSearch']),
  network: new Set(['getNetworkStats']),
  health: new Set(['health']),
} as const satisfies Record<string, ReadonlySet<string>>;

export type Resource = keyof typeof SDK_IDS_BY_RESOURCE;

const isGeneratedKeyHead = (head: unknown): head is { _id: string } =>
  typeof head === 'object' &&
  head !== null &&
  '_id' in head &&
  typeof (head as { _id?: unknown })._id === 'string';

export const matchResource =
  (resource: Resource) =>
  (queryKey: readonly unknown[]): boolean => {
    const head = queryKey[0];
    if (!isGeneratedKeyHead(head)) return false;
    return SDK_IDS_BY_RESOURCE[resource].has(head._id);
  };

export const invalidateResource = (
  queryClient: QueryClient,
  resource: Resource
) =>
  queryClient.invalidateQueries({
    predicate: (query) => matchResource(resource)(query.queryKey),
  });
