import { lazy, Suspense, type ComponentType, type ReactNode } from 'react';
import { createBrowserRouter } from 'react-router-dom';

import { DetailSkeleton } from '@rumblefish/soroban-block-explorer-ui';

import { PageStub } from '../pages/PageStub.js';
import { AccountDetailSkeleton } from '../pages/accounts/AccountDetailSkeleton.js';
import { AssetDetailSkeleton } from '../pages/assets/AssetDetailSkeleton.js';
import { ContractDetailSkeleton } from '../pages/contracts/ContractDetailSkeleton.js';
import { ListPageSkeleton } from '../pages/detail/ListPageSkeleton.js';
import { HomeSkeleton } from '../pages/home/HomeSkeleton.js';
import { LedgerDetailSkeleton } from '../pages/ledgers/LedgerDetailSkeleton.js';
import { NftDetailSkeleton } from '../pages/nft-detail/NftDetailSkeleton.js';
import { PoolDetailSkeleton } from '../pages/pool-detail/PoolDetailSkeleton.js';
import { TransactionDetailSkeleton } from '../pages/transaction-detail/TransactionDetailSkeleton.js';

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
        element: page(
          () => import('../pages/TransactionDetailPage.js'),
          <TransactionDetailSkeleton />
        ),
      },

      {
        path: 'ledgers',
        element: page(
          () => import('../pages/LedgersListPage.js'),
          <ListPageSkeleton showFilters={false} />
        ),
      },
      {
        path: 'ledgers/:sequence',
        element: page(
          () => import('../pages/LedgerDetailPage.js'),
          <LedgerDetailSkeleton />
        ),
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
        element: page(
          () => import('../pages/AccountDetailPage.js'),
          <AccountDetailSkeleton />
        ),
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
        element: page(
          () => import('../pages/AssetDetailPage.js'),
          <AssetDetailSkeleton />
        ),
      },

      {
        path: 'contracts',
        element: <PageStub title="Contracts" path="/contracts" />,
      },
      {
        path: 'contracts/:contractId',
        element: page(
          () => import('../pages/ContractDetailPage.js'),
          <ContractDetailSkeleton />
        ),
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
        element: page(
          () => import('../pages/NftDetailPage.js'),
          <NftDetailSkeleton />
        ),
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
        element: page(
          () => import('../pages/LiquidityPoolDetailPage.js'),
          <PoolDetailSkeleton />
        ),
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
