import { lazy, Suspense, type ComponentType } from 'react';
import { createBrowserRouter } from 'react-router-dom';

import { DetailSkeleton } from '@rumblefish/soroban-block-explorer-ui';

import { PageStub } from '../pages/PageStub.js';

import { AppShell } from './AppShell.js';
import { RouteErrorBoundary } from './RouteErrorBoundary.js';

const page = (load: () => Promise<{ default: ComponentType }>) => {
  const C = lazy(load);
  return (
    <Suspense fallback={<DetailSkeleton />}>
      <C />
    </Suspense>
  );
};

export const router = createBrowserRouter([
  {
    path: '/',
    element: <AppShell />,
    errorElement: <RouteErrorBoundary />,
    children: [
      { index: true, element: page(() => import('../pages/HomePage.js')) },

      {
        path: 'transactions',
        element: page(() => import('../pages/TransactionsListPage.js')),
      },
      {
        path: 'transactions/:hash',
        element: page(() => import('../pages/TransactionDetailPage.js')),
      },

      {
        path: 'ledgers',
        element: page(() => import('../pages/LedgersListPage.js')),
      },
      {
        path: 'ledgers/:sequence',
        element: page(() => import('../pages/LedgerDetailPage.js')),
      },

      {
        path: 'accounts',
        element: page(() => import('../pages/AccountsListPage.js')),
      },
      {
        path: 'accounts/:accountId',
        element: page(() => import('../pages/AccountDetailPage.js')),
      },

      {
        path: 'assets',
        element: page(() => import('../pages/AssetsListPage.js')),
      },
      {
        path: 'assets/:id',
        element: page(() => import('../pages/AssetDetailPage.js')),
      },

      {
        path: 'contracts',
        element: <PageStub title="Contracts" path="/contracts" />,
      },
      {
        path: 'contracts/:contractId',
        element: page(() => import('../pages/ContractDetailPage.js')),
      },

      { path: 'nfts', element: page(() => import('../pages/NftsListPage.js')) },
      {
        path: 'nfts/:contractId/:tokenId',
        element: page(() => import('../pages/NftDetailPage.js')),
      },

      {
        path: 'liquidity-pools',
        element: page(() => import('../pages/LiquidityPoolsListPage.js')),
      },
      {
        path: 'liquidity-pools/:id',
        element: page(() => import('../pages/LiquidityPoolDetailPage.js')),
      },

      {
        path: 'search',
        element: page(() => import('../pages/SearchResultsPage.js')),
      },
    ],
  },
]);
