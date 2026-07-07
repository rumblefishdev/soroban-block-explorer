import { lazy, Suspense, type ComponentType, type ReactNode } from 'react';
import { createBrowserRouter } from 'react-router-dom';

import {
  DetailSkeleton,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  routeSegments,
} from '@rumblefish/soroban-block-explorer-ui';
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
        path: routeSegments.transaction,
        element: page(
          () => import('../pages/TransactionsListPage.js'),
          <ListPageSkeleton />
        ),
      },
      {
        path: `${routeSegments.transaction}/:hash`,
        element: page(
          () => import('../pages/TransactionDetailPage.js'),
          <TransactionDetailSkeleton />
        ),
      },

      {
        path: routeSegments.ledger,
        element: page(
          () => import('../pages/LedgersListPage.js'),
          <ListPageSkeleton
            showFilters={false}
            rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
          />
        ),
      },
      {
        path: `${routeSegments.ledger}/:sequence`,
        element: page(
          () => import('../pages/LedgerDetailPage.js'),
          <LedgerDetailSkeleton />
        ),
      },

      {
        path: routeSegments.account,
        element: page(
          () => import('../pages/AccountsListPage.js'),
          <ListPageSkeleton />
        ),
      },
      {
        path: `${routeSegments.account}/:accountId`,
        element: page(
          () => import('../pages/AccountDetailPage.js'),
          <AccountDetailSkeleton />
        ),
      },

      {
        path: routeSegments.asset,
        element: page(
          () => import('../pages/AssetsListPage.js'),
          <ListPageSkeleton rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL} />
        ),
      },
      {
        path: `${routeSegments.asset}/:id`,
        element: page(
          () => import('../pages/AssetDetailPage.js'),
          <AssetDetailSkeleton />
        ),
      },

      {
        path: routeSegments.contract,
        element: page(
          () => import('../pages/ContractsListPage.js'),
          <ListPageSkeleton />
        ),
      },
      {
        path: `${routeSegments.contract}/:contractId`,
        element: page(
          () => import('../pages/ContractDetailPage.js'),
          <ContractDetailSkeleton />
        ),
      },

      {
        path: routeSegments.nft,
        element: page(
          () => import('../pages/NftsListPage.js'),
          <ListPageSkeleton rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL} />
        ),
      },
      {
        path: `${routeSegments.nft}/:contractId/:tokenId`,
        element: page(
          () => import('../pages/NftDetailPage.js'),
          <NftDetailSkeleton />
        ),
      },

      {
        path: routeSegments.pool,
        element: page(
          () => import('../pages/LiquidityPoolsListPage.js'),
          <ListPageSkeleton rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL} />
        ),
      },
      {
        path: `${routeSegments.pool}/:id`,
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
