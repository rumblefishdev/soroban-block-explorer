import { Box, Stack, Typography } from '@mui/material';
import {
  CardSkeleton,
  DetailErrorState,
  getDefaultTruncation,
  isAccountId,
  NotFoundState,
  SectionErrorBoundary,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { useParams } from 'react-router-dom';

import { useAccountDetail } from '../api/index.js';

import { AccountBalances } from './accounts/AccountBalances.js';
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

  let summary: ReactNode = null;
  let balances: ReactNode = null;
  if (account.isLoading) {
    summary = <CardSkeleton />;
    balances = <CardSkeleton />;
  } else if (account.isError) {
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
    balances = <AccountBalances balances={account.data.balances} />;
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
        <Typography variant="heading5SemiBold" component="h1">
          Account
        </Typography>
        <Typography
          variant="bodyMedium"
          sx={(theme) => ({
            color: theme.palette.text.secondary,
            wordBreak: 'break-all',
          })}
        >
          {accountId}
        </Typography>
      </Box>

      <SectionErrorBoundary sectionName="account-summary">
        {summary}
      </SectionErrorBoundary>
      {balances != null && (
        <SectionErrorBoundary sectionName="account-balances">
          {balances}
        </SectionErrorBoundary>
      )}
      {!account.isError && (
        <SectionErrorBoundary sectionName="account-transactions">
          <AccountTransactions accountId={accountId} />
        </SectionErrorBoundary>
      )}
    </Stack>
  );
}
