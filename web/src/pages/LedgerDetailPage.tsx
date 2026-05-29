import { Box, Link, Stack, Typography } from '@mui/material';
import {
  classifyError,
  DetailSkeleton,
  formatInteger,
  GenericErrorState,
  isLedgerSequence,
  isMissingResource,
  NotFoundState,
  RateLimitState,
  SectionErrorBoundary,
  TransientErrorState,
  useCursorPagination,
  usePageHandlers,
} from '@rumblefish/soroban-block-explorer-ui';
import { Link as RouterLink, useParams } from 'react-router-dom';

import { useLedgerDetail } from '../api/index.js';
import { routes } from '../router/routes.js';

import { LedgerNav } from './ledgers/LedgerNav.js';
import { LedgerSummary } from './ledgers/LedgerSummary.js';
import { LedgerTransactions } from './ledgers/LedgerTransactions.js';

export default function LedgerDetailPage() {
  const { sequence: rawSequence } = useParams<{ sequence: string }>();
  const valid = rawSequence != null && isLedgerSequence(rawSequence);
  const sequence = valid ? Number(rawSequence) : Number.NaN;

  // Cursors are scoped to a specific ledger's embedded transactions, so
  // navigating to a different ledger (e.g. via LedgerNav prev/next) must
  // drop any cursor lingering in the URL.
  const { cursor, goNext, goPrev } = useCursorPagination({
    resetKey: sequence,
  });

  const { data, isLoading, isError, error, refetch } = useLedgerDetail(
    sequence,
    cursor,
    valid
  );

  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.transactions.page,
    goNext,
    goPrev
  );

  if (!valid) {
    return <NotFoundState entity="ledger" identifier={rawSequence} />;
  }

  if (isLoading) {
    return <DetailSkeleton />;
  }

  if (isError) {
    const kind = classifyError(error);
    if (isMissingResource(kind)) {
      // Backend returns 400 INVALID_SEQUENCE for i64-overflow values
      // (e.g. `/ledgers/99999999999`); 404 for in-range sequences with
      // no record. Both are "this ledger isn't here" from the user's
      // POV — single NotFound state (task 0251 H8).
      return <NotFoundState entity="ledger" identifier={rawSequence} />;
    }
    const retry = () => void refetch();
    return kind === 'rate-limit' ? (
      <RateLimitState onRetry={retry} py={8} />
    ) : kind === 'transient' ? (
      <TransientErrorState onRetry={retry} py={8} />
    ) : (
      <GenericErrorState onRetry={retry} py={8} />
    );
  }

  if (!data) {
    return <NotFoundState entity="ledger" identifier={rawSequence} />;
  }

  const ledger = data;
  const txRows = ledger.transactions.data;
  const sequenceLabel = formatInteger(ledger.sequence);

  return (
    <Stack spacing={3}>
      <Box>
        <Box sx={{ display: 'flex', gap: 0.5, mb: 1 }}>
          <Link
            component={RouterLink}
            to={routes.ledgers}
            variant="bodySmMedium"
            underline="hover"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            Ledger
          </Link>
          <Typography
            variant="bodySmMedium"
            sx={(theme) => ({ color: theme.palette.text.tertiary })}
          >
            /
          </Typography>
          <Typography
            variant="bodySmMedium"
            sx={(theme) => ({ color: theme.palette.text.primary })}
          >
            {sequenceLabel}
          </Typography>
        </Box>
        <Box
          sx={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            gap: 2,
            flexWrap: 'wrap',
          }}
        >
          <Typography variant="heading5SemiBold" component="h1">
            Ledger {sequenceLabel}
          </Typography>
          <LedgerNav
            prevSequence={ledger.prev_sequence}
            nextSequence={ledger.next_sequence}
          />
        </Box>
      </Box>

      <SectionErrorBoundary sectionName="Ledger summary">
        <LedgerSummary ledger={ledger} />
      </SectionErrorBoundary>

      <SectionErrorBoundary sectionName="Ledger transactions">
        <LedgerTransactions
          rows={txRows}
          totalCount={ledger.transaction_count}
          canPrev={canPrev}
          canNext={canNext}
          onPrev={handlePrev}
          onNext={handleNext}
        />
      </SectionErrorBoundary>
    </Stack>
  );
}
