import { Box, Card, Stack, Typography } from '@mui/material';
import {
  CardSkeleton,
  Chip,
  classifyError,
  GenericErrorState,
  isContractId,
  isMissingResource,
  NotFoundState,
  SectionErrorBoundary,
  Tabs,
  truncateMiddle,
  useTabUrlState,
  type TabDefinition,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';
import { useParams } from 'react-router-dom';

import { useContractDetail } from '../api/index.js';

import { ContractEvents } from './contracts/ContractEvents.js';
import { ContractInterface } from './contracts/ContractInterface.js';
import { ContractInvocations } from './contracts/ContractInvocations.js';
import { ContractSummary } from './contracts/ContractSummary.js';
import { PageBreadcrumb } from './detail/PageBreadcrumb.js';

// Breadcrumb crumb uses tighter truncation than the DS default for contracts —
// matches the existing AccountDetailPage breadcrumb (4 / 4 instead of 6 / 4).
const BREADCRUMB_TRUNCATION = { prefix: 4, suffix: 4 } as const;

const TAB_KEYS = ['interface', 'invocations', 'events'] as const;

/**
 * Contract detail page (`/contracts/:contractId`) — summary with windowed
 * stats, then tabbed Interface / Invocations / Events sections. Each section
 * fetches independently so one failing query never collapses the others;
 * the active tab is mirrored in the URL `?tab=` query param.
 */
export default function ContractDetailPage() {
  const { contractId = '' } = useParams<{ contractId: string }>();
  const valid = isContractId(contractId);
  const contract = useContractDetail(valid ? contractId : '');
  const { activeKey, setActiveKey } = useTabUrlState({
    defaultKey: 'interface',
    validKeys: TAB_KEYS,
  });

  if (!valid) {
    return (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 8 }}>
        <NotFoundState entity="contract" identifier={contractId} />
      </Box>
    );
  }

  let summary: ReactNode = null;
  if (contract.isLoading) {
    summary = <CardSkeleton />;
  } else if (contract.isError) {
    summary = (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 6 }}>
        {isMissingResource(classifyError(contract.error)) ? (
          <NotFoundState entity="contract" identifier={contractId} />
        ) : (
          <GenericErrorState onRetry={() => void contract.refetch()} />
        )}
      </Box>
    );
  } else if (contract.data) {
    summary = <ContractSummary contract={contract.data} />;
  }

  // Count pills are intentionally omitted for now — the API has no honest
  // per-tab totals (no event count at all; invocation/function counts are
  // windowed or query-dependent).
  const tabs: TabDefinition[] = [
    { key: 'interface', label: 'Interface' },
    { key: 'invocations', label: 'Invocations' },
    { key: 'events', label: 'Events' },
  ];

  return (
    <Stack spacing={3}>
      <Box>
        <PageBreadcrumb
          items={[
            { label: 'Contract' },
            { label: truncateMiddle(contractId, BREADCRUMB_TRUNCATION) },
          ]}
        />
        <Stack
          direction="row"
          spacing={1.5}
          alignItems="center"
          sx={{ flexWrap: 'wrap' }}
        >
          <Typography variant="heading3SemiBold" component="h1">
            Contract
          </Typography>
          {contract.data?.is_sac === true && (
            <Chip size="md" color="accent" label="Stellar Asset Contract" />
          )}
        </Stack>
        <Typography
          variant="bodyMonoSmRegular"
          sx={{ color: 'text.secondary', wordBreak: 'break-all' }}
        >
          {contractId}
        </Typography>
      </Box>

      <SectionErrorBoundary sectionName="contract-summary">
        {summary}
      </SectionErrorBoundary>

      <Card>
        <Box
          sx={(theme) => ({
            // Tab bar sits on the darker surface (Figma "Table sections").
            backgroundColor: theme.palette.surface.grayMainAlt,
            borderBottom: `1px solid ${theme.palette.stroke.default}`,
            px: 1,
          })}
        >
          <Tabs
            tabs={tabs}
            activeKey={activeKey}
            onChange={setActiveKey}
            aria-label="Contract sections"
          />
        </Box>
        <SectionErrorBoundary
          key={activeKey}
          sectionName={`contract-${activeKey}`}
        >
          {activeKey === 'interface' && (
            <ContractInterface contractId={contractId} />
          )}
          {activeKey === 'invocations' && (
            <ContractInvocations contractId={contractId} />
          )}
          {activeKey === 'events' && <ContractEvents contractId={contractId} />}
        </SectionErrorBoundary>
      </Card>
    </Stack>
  );
}
