import { Box, Stack, Typography } from '@mui/material';
import {
  Chip,
  DetailErrorState,
  getDefaultTruncation,
  NotFoundState,
  SectionErrorBoundary,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';

import { useTransactionDetail } from '../../api/index.js';
import { routes } from '../../router/routes.js';
import { PageBreadcrumb } from '../detail/PageBreadcrumb.js';

import { classifyTx } from './shared/classifyTx.js';
import { EventsSection } from './sections/EventsSection.js';
import { RawDataSection } from './sections/RawDataSection.js';
import { OperationsSection } from './sections/OperationsSection.js';
import { SignaturesTable } from './sections/SignaturesTable.js';
import { TransactionSummary } from './sections/TransactionSummary.js';
import { TransactionDetailSkeleton } from './TransactionDetailSkeleton.js';
import { useSelectedOp } from './useSelectedOp.js';
import { useTxHashParam } from './useTxHashParam.js';

export default function TransactionDetailPage() {
  const { hash, valid } = useTxHashParam();
  const [selectedIndex, setSelectedIndex] = useSelectedOp();
  const query = useTransactionDetail(valid ? hash : '');

  if (!valid) {
    return <NotFoundState entity="transaction" identifier={hash} />;
  }

  if (query.isLoading) {
    return <TransactionDetailSkeleton />;
  }

  if (query.isError) {
    return (
      <DetailErrorState
        error={query.error}
        entity="transaction"
        identifier={hash}
        onRetry={() => void query.refetch()}
        py={8}
      />
    );
  }

  if (query.data == null) return null;
  const tx = query.data;
  const heavy = tx.heavy ?? null;
  // `heavy_fields_status === 'unavailable'` iff `heavy` is null (wire contract),
  // so the null check is the status check. Sections that live only in `heavy`
  // must say "could not load" rather than count a `?? []` fallback (0377 F1/F2).
  const heavyUnavailable = heavy == null;
  // The one-line story ("Swap · 4 ops") belongs to the page title — next to
  // the Success/Failed chip it read as a second STATUS (0460 #9).
  const story = classifyTx(tx);

  return (
    <Stack spacing={3}>
      <Box>
        <PageBreadcrumb
          items={[
            { label: 'Transactions', to: routes.transactions },
            {
              label: truncateMiddle(hash, getDefaultTruncation('transaction')),
            },
          ]}
        />
        <Stack direction="row" spacing={1.5} alignItems="center">
          <Typography variant="heading5SemiBold" component="h1">
            Transaction Detail
          </Typography>
          {story != null && <Chip size="sm" color="neutral" label={story} />}
        </Stack>
      </Box>

      <SectionErrorBoundary sectionName="transaction-summary">
        <TransactionSummary tx={tx} />
      </SectionErrorBoundary>

      <SectionErrorBoundary sectionName="transaction-operations">
        <OperationsSection
          tx={tx}
          selectedIndex={selectedIndex}
          onSelect={setSelectedIndex}
        />
      </SectionErrorBoundary>

      <SectionErrorBoundary sectionName="transaction-signatures">
        <SignaturesTable signatures={heavy?.signatures ?? []} />
      </SectionErrorBoundary>

      {/* One progressive view (0453): the former advanced-only sections render
          always — Events collapsed by default, raw XDR already collapses per
          row — so nothing the old toggle gated is lost. */}
      <SectionErrorBoundary sectionName="transaction-events">
        <EventsSection
          contractEvents={heavy?.contract_events ?? []}
          diagnosticEvents={heavy?.diagnostic_events ?? []}
          unavailable={heavyUnavailable}
        />
      </SectionErrorBoundary>
      <SectionErrorBoundary sectionName="transaction-raw-data">
        <RawDataSection
          envelopeXdr={heavy?.envelope_xdr}
          resultXdr={heavy?.result_xdr}
          resultMetaXdr={heavy?.result_meta_xdr}
          unavailable={heavyUnavailable}
        />
      </SectionErrorBoundary>
    </Stack>
  );
}
