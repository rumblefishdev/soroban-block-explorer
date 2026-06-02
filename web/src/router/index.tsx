import { lazy, Suspense, type ComponentType, type ReactNode } from 'react';
import { createBrowserRouter } from 'react-router-dom';

import { DetailSkeleton } from '@rumblefish/soroban-block-explorer-ui';

import { PageStub } from '../pages/PageStub.js';
import { ListPageSkeleton } from '../pages/detail/ListPageSkeleton.js';
import { HomeSkeleton } from '../pages/home/HomeSkeleton.js';

import { AppShell } from './AppShell.js';
import { RouteErrorBoundary } from './RouteErrorBoundary.js';

const page = (
  load: () => Promise<{ default: ComponentType }>,
  fallback: ReactNode = <DetailSkeleton />
) => {
  const C = lazy(load);
  return (
    <Suspense fallback={fallback}>
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
      {
        index: true,
        element: page(() => import('../pages/HomePage.js'), <HomeSkeleton />),
      },

      {
        path: 'transactions',
        element: page(
          () => import('../pages/TransactionsListPage.js'),
          <ListPageSkeleton />
        ),
      },
      {
        path: 'transactions/:hash',
        element: page(() => import('../pages/TransactionDetailPage.js')),
      },

      {
        path: 'ledgers',
        element: page(
          () => import('../pages/LedgersListPage.js'),
          <ListPageSkeleton />
        ),
      },
      {
        path: 'ledgers/:sequence',
        element: page(() => import('../pages/LedgerDetailPage.js')),
      },

      {
        path: 'accounts',
        element: page(
          () => import('../pages/AccountsListPage.js'),
          <ListPageSkeleton />
        ),
      },
      {
        path: 'accounts/:accountId',
        element: page(() => import('../pages/AccountDetailPage.js')),
      },

      {
        path: 'assets',
        element: page(
          () => import('../pages/AssetsListPage.js'),
          <ListPageSkeleton />
        ),
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

      {
        path: 'nfts',
        element: page(
          () => import('../pages/NftsListPage.js'),
          <ListPageSkeleton />
        ),
      },
      {
        path: 'nfts/:contractId/:tokenId',
        element: page(() => import('../pages/NftDetailPage.js')),
      },

      {
        path: 'liquidity-pools',
        element: page(
          () => import('../pages/LiquidityPoolsListPage.js'),
          <ListPageSkeleton />
        ),
      },
      {
        path: 'liquidity-pools/:id',
        element: page(() => import('../pages/LiquidityPoolDetailPage.js')),
      },

      {
        path: 'search',
        element: page(() => import('../pages/SearchResultsPage.js')),
      },
      {
        path: '*',
        element: page(() => import('../pages/NotFoundPage.js')),
      },
    ],
  },
]);
