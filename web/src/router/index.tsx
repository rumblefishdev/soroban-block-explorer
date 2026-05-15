import { lazy, Suspense, type ComponentType } from 'react';
import { createBrowserRouter } from 'react-router-dom';

import { DetailSkeleton } from '@rumblefish/soroban-block-explorer-ui';

import { AppShellStub } from './AppShellStub.js';
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
    element: <AppShellStub />,
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
        path: 'accounts/:accountId',
        element: page(() => import('../pages/AccountDetailPage.js')),
      },

      {
        path: 'tokens',
        element: page(() => import('../pages/TokensListPage.js')),
      },
      {
        path: 'tokens/:id',
        element: page(() => import('../pages/TokenDetailPage.js')),
      },

      {
        path: 'contracts/:contractId',
        element: page(() => import('../pages/ContractDetailPage.js')),
      },

      { path: 'nfts', element: page(() => import('../pages/NftsListPage.js')) },
      {
        path: 'nfts/:id',
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
