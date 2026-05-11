import type { QueryClient } from '@tanstack/react-query';

const SDK_IDS_BY_RESOURCE = {
  transactions: [
    'listTransactions',
    'getTransaction',
    'listAccountTransactions',
    'listAssetTransactions',
    'listPoolTransactions',
  ],
  accounts: ['getAccount', 'listAccountTransactions'],
  ledgers: ['listLedgers', 'getLedger'],
  assets: ['listAssets', 'getAsset', 'listAssetTransactions'],
  contracts: ['getContract', 'getInterface', 'listInvocations', 'listEvents'],
  nfts: ['listNfts', 'getNft', 'listNftTransfers'],
  pools: [
    'listPools',
    'getPool',
    'getPoolChart',
    'listParticipants',
    'listPoolTransactions',
  ],
  search: ['getSearch'],
  network: ['getNetworkStats'],
  health: ['health'],
} as const satisfies Record<string, readonly string[]>;

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
    const ids = SDK_IDS_BY_RESOURCE[resource] as readonly string[];
    return ids.includes(head._id);
  };

export const invalidateResource = (
  queryClient: QueryClient,
  resource: Resource
) =>
  queryClient.invalidateQueries({
    predicate: (query) => matchResource(resource)(query.queryKey),
  });
