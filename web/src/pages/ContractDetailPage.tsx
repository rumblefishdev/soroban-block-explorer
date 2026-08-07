import { Box, Card, Stack, Typography } from '@mui/material';
import {
  Chip,
  DetailErrorState,
  IdentifierWithCopy,
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

import { ContractCode } from './contracts/ContractCode.js';
import { ContractEvents } from './contracts/ContractEvents.js';
import { ContractInterface } from './contracts/ContractInterface.js';
import { ContractInvocations } from './contracts/ContractInvocations.js';
import { ContractDetailSkeleton } from './contracts/ContractDetailSkeleton.js';
import { ContractSummary } from './contracts/ContractSummary.js';
import { PageBreadcrumb } from './detail/PageBreadcrumb.js';

const BREADCRUMB_TRUNCATION = { prefix: 4, suffix: 4 } as const;

const TAB_KEYS = ['interface', 'code', 'invocations', 'events'] as const;

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

  // The Code tab (task 0465) only exists for contracts with a WASM — SAC
  // and pre-upload contracts have nothing to decompile by design. A stale
  // `?tab=code` URL on such a contract falls back to Interface rather than
  // rendering an empty pane.
  const hasWasm = contract.data?.wasm_hash != null;
  const effectiveKey =
    activeKey === 'code' && !hasWasm ? 'interface' : activeKey;

  // No count badges on the Invocations / Events tabs (task 0348 F1):
  // `recent_invocations` / `recent_events` are a 7-day activity window, but a
  // tab badge reads as "items in this tab" — and the tabs' tables are
  // all-time. On the 84.6% of contracts dormant >7d the badge showed "0"
  // over a full table. The honest 7-day figure stays on the KPI cards, which
  // are explicitly labelled "(last 7 days)".
  const tabs: TabDefinition[] = [
    { key: 'interface', label: 'Interface' },
    ...(hasWasm
      ? [
          {
            key: 'code',
            // Amber dot = experimental marker (matches the in-tab chip).
            label: (
              <Box
                component="span"
                sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.75 }}
              >
                Code
                <Box
                  component="span"
                  sx={{
                    width: 6,
                    height: 6,
                    borderRadius: '50%',
                    bgcolor: 'warning.main',
                  }}
                />
              </Box>
            ),
          } satisfies TabDefinition,
        ]
      : []),
    { key: 'invocations', label: 'Invocations' },
    { key: 'events', label: 'Events' },
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
          {/* Task 0327 — mutability, 3-state; null/undefined (Unknown) → no chip.
              Label states exactly what the WASM import scan proves ("self-
              upgrade path present/absent"), not the broader "immutable" — a
              static scan can't see proxy/delegate or renounced-admin patterns. */}
          {contract.data?.upgradeable != null && (
            <Chip
              size="md"
              color={contract.data.upgradeable ? 'emerald' : 'neutral'}
              label={
                contract.data.upgradeable
                  ? 'Self-upgradeable'
                  : 'No self-upgrade'
              }
            />
          )}
        </Stack>
        {/* Truncated under-title identity (full id stays in the summary
            card below); the special identifier component carries the copy
            affordance. */}
        <IdentifierWithCopy value={contractId} type="contract" linked={false} />
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
              activeKey={effectiveKey}
              onChange={setActiveKey}
              aria-label="Contract sections"
            />
          </Box>
          <SectionErrorBoundary
            key={effectiveKey}
            sectionName={`contract-${effectiveKey}`}
          >
            {effectiveKey === 'interface' && (
              <ContractInterface contractId={contractId} />
            )}
            {effectiveKey === 'code' && hasWasm && (
              <ContractCode contractId={contractId} />
            )}
            {effectiveKey === 'invocations' && (
              <ContractInvocations contractId={contractId} />
            )}
            {effectiveKey === 'events' && (
              <ContractEvents contractId={contractId} />
            )}
          </SectionErrorBoundary>
        </Card>
      )}
    </Stack>
  );
}
