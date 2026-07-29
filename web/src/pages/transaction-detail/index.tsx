import { Box, Stack, Typography } from '@mui/material';
import {
  DetailErrorState,
  getDefaultTruncation,
  NotFoundState,
  SectionErrorBoundary,
  truncateMiddle,
} from '@rumblefish/soroban-block-explorer-ui';

import { useTransactionDetail } from '../../api/index.js';
import { routes } from '../../router/routes.js';
import { PageBreadcrumb } from '../detail/PageBreadcrumb.js';

import { EventsSection } from './advanced/EventsSection.js';
import { RawDataSection } from './advanced/RawDataSection.js';
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
        <Typography variant="heading5SemiBold" component="h1">
          Transaction Detail
        </Typography>
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
        />
      </SectionErrorBoundary>
      <SectionErrorBoundary sectionName="transaction-raw-data">
        <RawDataSection
          envelopeXdr={heavy?.envelope_xdr}
          resultXdr={heavy?.result_xdr}
        />
      </SectionErrorBoundary>
    </Stack>
  );
}
