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
import { ModeToggle } from './sections/ModeToggle.js';
import { OperationsSection } from './sections/OperationsSection.js';
import { SignaturesTable } from './sections/SignaturesTable.js';
import { TransactionSummary } from './sections/TransactionSummary.js';
import { TransactionDetailSkeleton } from './TransactionDetailSkeleton.js';
import { useDetailMode } from './useDetailMode.js';
import { useSelectedOp } from './useSelectedOp.js';
import { useTxHashParam } from './useTxHashParam.js';

export default function TransactionDetailPage() {
  const { hash, valid } = useTxHashParam();
  const { mode, setMode } = useDetailMode();
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
        <Stack
          direction="row"
          alignItems="center"
          justifyContent="space-between"
          spacing={2}
          sx={{ flexWrap: 'wrap' }}
        >
          <Typography variant="heading5SemiBold" component="h1">
            Transaction Detail
          </Typography>
          <ModeToggle mode={mode} onChange={setMode} />
        </Stack>
      </Box>

      <SectionErrorBoundary sectionName="transaction-summary">
        <TransactionSummary tx={tx} />
      </SectionErrorBoundary>

      <SectionErrorBoundary sectionName="transaction-operations">
        <OperationsSection
          tx={tx}
          mode={mode}
          selectedIndex={selectedIndex}
          onSelect={setSelectedIndex}
        />
      </SectionErrorBoundary>

      <SectionErrorBoundary sectionName="transaction-signatures">
        <SignaturesTable signatures={heavy?.signatures ?? []} />
      </SectionErrorBoundary>

      {mode === 'advanced' && (
        <>
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
        </>
      )}
    </Stack>
  );
}
