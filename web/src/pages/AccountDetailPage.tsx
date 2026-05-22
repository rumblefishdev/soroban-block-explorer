import { Box, Stack, Typography } from '@mui/material';
import {
  CardSkeleton,
  classifyError,
  GenericErrorState,
  isAccountId,
  isMissingResource,
  NotFoundState,
  SectionErrorBoundary,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { useParams } from 'react-router-dom';

import { useAccountDetail } from '../api/index.js';

import { AccountBalances } from './accounts/AccountBalances.js';
import { AccountSummary } from './accounts/AccountSummary.js';
import { AccountTransactions } from './accounts/AccountTransactions.js';
import { PageBreadcrumb } from './detail/PageBreadcrumb.js';

/** `GDQP…EE36`-style short form for the breadcrumb crumb. */
function shortId(id: string): string {
  return id.length > 12 ? `${id.slice(0, 4)}…${id.slice(-4)}` : id;
}

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
    return (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 8 }}>
        <NotFoundState entity="account" identifier={accountId} />
      </Box>
    );
  }

  let summary: ReactNode = null;
  let balances: ReactNode = null;
  if (account.isLoading) {
    summary = <CardSkeleton />;
    balances = <CardSkeleton />;
  } else if (account.isError) {
    summary = (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 6 }}>
        {isMissingResource(classifyError(account.error)) ? (
          <NotFoundState entity="account" identifier={accountId} />
        ) : (
          <GenericErrorState onRetry={() => void account.refetch()} />
        )}
      </Box>
    );
  } else if (account.data) {
    summary = <AccountSummary account={account.data} />;
    balances = <AccountBalances balances={account.data.balances} />;
  }

  return (
    <Stack spacing={3}>
      <Box>
        <PageBreadcrumb
          items={[{ label: 'Account' }, { label: shortId(accountId) }]}
        />
        <Typography variant="heading3SemiBold" component="h1">
          Account
        </Typography>
        <Typography
          variant="bodyMonoSmRegular"
          sx={{ color: 'text.secondary', wordBreak: 'break-all' }}
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
      <SectionErrorBoundary sectionName="account-transactions">
        <AccountTransactions accountId={accountId} />
      </SectionErrorBoundary>
    </Stack>
  );
}
