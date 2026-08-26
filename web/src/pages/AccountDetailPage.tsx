import { Box, Stack, Typography } from '@mui/material';
import {
  Chip,
  DetailErrorState,
  getDefaultTruncation,
  IdentifierWithCopy,
  isAccountId,
  NotFoundState,
  SectionErrorBoundary,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { useParams } from 'react-router-dom';

import { useAccountDetail } from '../api/index.js';

import { AccountBalances } from './accounts/AccountBalances.js';
import { AccountDetailSkeleton } from './accounts/AccountDetailSkeleton.js';
import { AccountSigners } from './accounts/AccountSigners.js';
import { AccountSummary } from './accounts/AccountSummary.js';
import { AccountTransactions } from './accounts/AccountTransactions.js';
import { PageBreadcrumb } from './detail/PageBreadcrumb.js';

/**
 * Account detail page (`/accounts/:accountId`) — summary, balances, and a
 * paginated transaction history. Summary/balances and transactions are
 * fetched by independent queries so one failing section never collapses the
 * others.
 */
export default function AccountDetailPage() {
  const { accountId = '' } = useParams<{ accountId: string }>();
  const valid = isAccountId(accountId);
  const account = useAccountDetail(valid ? accountId : '');

  if (!valid) {
    return <NotFoundState entity="account" identifier={accountId} />;
  }

  if (account.isLoading) {
    return <AccountDetailSkeleton />;
  }

  let summary: ReactNode = null;
  let balances: ReactNode = null;
  let signers: ReactNode = null;
  if (account.isError) {
    summary = (
      <DetailErrorState
        error={account.error}
        entity="account"
        identifier={accountId}
        onRetry={() => void account.refetch()}
      />
    );
  } else if (account.data) {
    summary = <AccountSummary account={account.data} />;
    balances = (
      <AccountBalances
        balances={account.data.balances}
        deleted={account.data.deleted}
      />
    );
    signers = (
      <AccountSigners
        accountId={accountId}
        signing={account.data.signing}
        // Classic only (type 0/1). A classic trustline cannot exist without
        // an AccountEntry; a Soroban token balance can — it lives in the
        // token contract's own storage, keyed by address, so it survives
        // account_merge and can even predate any account at all.
        hasClassicHoldings={account.data.balances.some(
          (b) => b.type === 0 || b.type === 1
        )}
        deleted={account.data.deleted}
      />
    );
  }

  return (
    <Stack spacing={3}>
      <Box>
        <PageBreadcrumb
          items={[
            { label: 'Account' },
            {
              label: truncateMiddle(accountId, getDefaultTruncation('account')),
            },
          ]}
        />
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          <Typography variant="heading5SemiBold" component="h1">
            Account
          </Typography>
          {/* Account removed from the ledger via account_merge (task 0324). */}
          {account.data?.deleted === true && (
            <Chip size="sm" color="error" dot label="Deleted" />
          )}
        </Box>
        {/* Truncated under-title identity (full id stays in the summary
            card below); the special identifier component carries the copy
            affordance. */}
        <IdentifierWithCopy value={accountId} type="account" linked={false} />
      </Box>

      <SectionErrorBoundary sectionName="account-summary">
        {summary}
      </SectionErrorBoundary>
      {balances != null && (
        <SectionErrorBoundary sectionName="account-balances">
          {balances}
        </SectionErrorBoundary>
      )}
      {signers != null && (
        <SectionErrorBoundary sectionName="account-signers">
          {signers}
        </SectionErrorBoundary>
      )}
      {/* Gate on resolved parent data (not just `!isError`) so the
          transactions query never fires while the parent is still loading —
          a parent 404 then produces zero sub-section 404s. */}
      {account.data != null && (
        <SectionErrorBoundary sectionName="account-transactions">
          <AccountTransactions accountId={accountId} />
        </SectionErrorBoundary>
      )}
    </Stack>
  );
}
