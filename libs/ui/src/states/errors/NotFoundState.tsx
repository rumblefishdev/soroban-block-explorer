import SearchOffIcon from '@mui/icons-material/SearchOff';
import type { ReactNode } from 'react';

import { EmptyState } from '../empty/EmptyState.js';

export type NotFoundEntity =
  | 'transaction'
  | 'account'
  | 'contract'
  | 'ledger'
  | 'operation'
  | 'asset'
  | 'liquidity-pool'
  | 'generic';

const TITLES: Record<NotFoundEntity, string> = {
  transaction: 'Transaction not found',
  account: 'Account not found',
  contract: 'Contract not found',
  ledger: 'Ledger not found',
  operation: 'Operation not found',
  asset: 'Asset not found',
  'liquidity-pool': 'Liquidity pool not found',
  generic: 'Not found',
};

interface NotFoundStateProps {
  entity?: NotFoundEntity;
  identifier?: string;
  action?: ReactNode;
  titleOverride?: string;
}

export function NotFoundState({
  entity = 'generic',
  identifier,
  action,
  titleOverride,
}: NotFoundStateProps) {
  return (
    <EmptyState
      icon={<SearchOffIcon />}
      title={titleOverride ?? TITLES[entity]}
      description="We couldn't find anything matching this identifier. Double-check the value and try again."
      meta={identifier}
      action={action}
    />
  );
}
