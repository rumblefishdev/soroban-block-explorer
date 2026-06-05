import { Box, Card, Stack, Typography } from '@mui/material';
import {
  Chip,
  DetailErrorState,
  isContractId,
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
import { routes } from '../router/routes.js';

import { ContractEvents } from './contracts/ContractEvents.js';
import { ContractInterface } from './contracts/ContractInterface.js';
import { ContractInvocations } from './contracts/ContractInvocations.js';
import { ContractDetailSkeleton } from './contracts/ContractDetailSkeleton.js';
import { ContractSummary } from './contracts/ContractSummary.js';
import { PageBreadcrumb } from './detail/PageBreadcrumb.js';

const BREADCRUMB_TRUNCATION = { prefix: 4, suffix: 4 } as const;

const TAB_KEYS = ['interface', 'invocations', 'events'] as const;

export default function ContractDetailPage() {
  const { contractId = '' } = useParams<{ contractId: string }>();
  const valid = isContractId(contractId);
  const contract = useContractDetail(valid ? contractId : '');
  const { activeKey, setActiveKey } = useTabUrlState({
    defaultKey: 'interface',
    validKeys: TAB_KEYS,
  });

  if (!valid) {
    return <NotFoundState entity="contract" identifier={contractId} />;
  }

  if (contract.isLoading) {
    return <ContractDetailSkeleton />;
  }

  let summary: ReactNode = null;
  if (contract.isError) {
    summary = (
      <DetailErrorState
        error={contract.error}
        entity="contract"
        identifier={contractId}
        onRetry={() => void contract.refetch()}
      />
    );
  } else if (contract.data) {
    summary = <ContractSummary contract={contract.data} />;
  }

  const tabs: TabDefinition[] = [
    { key: 'interface', label: 'Interface' },
    {
      key: 'invocations',
      label: 'Invocations',
      count: contract.data?.stats.recent_invocations,
    },
    {
      key: 'events',
      label: 'Events',
      count: contract.data?.stats.recent_events,
    },
  ];

  return (
    <Stack spacing={3}>
      <Box>
        <PageBreadcrumb
          items={[
            { label: 'Contracts', to: routes.contracts },
            { label: truncateMiddle(contractId, BREADCRUMB_TRUNCATION) },
          ]}
        />
        <Stack
          direction="row"
          spacing={1.5}
          alignItems="center"
          sx={{ flexWrap: 'wrap' }}
        >
          <Typography variant="heading5SemiBold" component="h1">
            Contract
          </Typography>
          {contract.data?.is_sac === true && (
            <Chip size="md" color="accent" label="Stellar Asset Contract" />
          )}
        </Stack>
        <Typography
          variant="bodyMedium"
          sx={(theme) => ({
            color: theme.palette.text.secondary,
            wordBreak: 'break-all',
          })}
        >
          {contractId}
        </Typography>
      </Box>

      <SectionErrorBoundary sectionName="contract-summary">
        {summary}
      </SectionErrorBoundary>

      {/* Gate the tabbed sub-sections on resolved parent data so their
          queries never fire while the contract is still loading — a parent
          404 then produces zero sub-section 404s. */}
      {contract.data != null && (
        <Card>
          <Box
            sx={(theme) => ({
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
            {activeKey === 'events' && (
              <ContractEvents contractId={contractId} />
            )}
          </SectionErrorBoundary>
        </Card>
      )}
    </Stack>
  );
}
