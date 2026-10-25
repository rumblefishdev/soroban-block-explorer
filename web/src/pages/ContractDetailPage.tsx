import { Box, Card, Stack, Tooltip, Typography } from '@mui/material';
import {
  CardSkeleton,
  Chip,
  DetailErrorState,
  getIdentifierHref,
  IdentifierWithCopy,
  isContractId,
  NotFoundState,
  QueryErrorState,
  SectionErrorBoundary,
  Tabs,
  truncateMiddle,
  unmuxed,
  useTabUrlState,
  type TabDefinition,
} from '@rumblefish/soroban-block-explorer-ui';
import { type ReactNode } from 'react';
import { Navigate, useLocation, useParams } from 'react-router-dom';

import { useContractDetail, useContractVerification } from '../api/index.js';
import { routes } from '../router/routes.js';

import { ContractCode } from './contracts/ContractCode.js';
import { contractFace } from './contracts/contractFace.js';
import { ContractFaceChip } from './contracts/ContractFaceChip.js';
import { ContractEvents } from './contracts/ContractEvents.js';
import { ContractInterface } from './contracts/ContractInterface.js';
import { ContractInvocations } from './contracts/ContractInvocations.js';
import { ContractDetailSkeleton } from './contracts/ContractDetailSkeleton.js';
import { ContractSummary } from './contracts/ContractSummary.js';
import {
  VerifiedCode,
  isShownVerification,
  verifiedByNames,
} from './contracts/VerifiedCode.js';
import { PageBreadcrumb } from './detail/PageBreadcrumb.js';

const BREADCRUMB_TRUNCATION = { prefix: 4, suffix: 4 } as const;

const TAB_KEYS = [
  'interface',
  'code',
  'verified',
  'invocations',
  'events',
] as const;

export default function ContractDetailPage() {
  const { contractId = '' } = useParams<{ contractId: string }>();
  const location = useLocation();
  const valid = isContractId(contractId);
  const contract = useContractDetail(valid ? contractId : '');
  const { activeKey, setActiveKey } = useTabUrlState({
    defaultKey: 'interface',
    validKeys: TAB_KEYS,
  });
  const face = contract.data != null ? contractFace(contract.data) : null;
  // Task 0639: asked with the page — it decides whether the Verified Code tab
  // and the header chip exist. Any answer but a verified one shows nothing.
  const verification = useContractVerification(
    contractId,
    valid && contract.data?.wasm_hash != null
  );
  const verified = isShownVerification(verification.data)
    ? verification.data
    : null;

  if (!valid) {
    // A `W…` (a contract with a sub-account id, CAP-0084) has its contract's
    // page; the open tab rides along.
    const base = unmuxed(contractId);
    if (base != null && isContractId(base))
      return (
        <Navigate
          to={getIdentifierHref('contract', base) + location.search}
          replace
        />
      );
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

  // The Decompilation tab (task 0465) only exists for contracts with a WASM — SAC
  // and pre-upload contracts have nothing to decompile by design. A stale
  // `?tab=code` URL on such a contract falls back to Interface rather than
  // rendering an empty pane.
  const hasWasm = contract.data?.wasm_hash != null;
  // A deep link to Verified Code waits for the answer instead of falling
  // back to Interface and jumping when it lands.
  const verificationPending =
    activeKey === 'verified' && hasWasm && verification.isPending;
  // …and one that fails (other than a 404: no such contract or no WASM)
  // shows the error with Retry rather than a silent fallback.
  const verificationFailed =
    activeKey === 'verified' &&
    hasWasm &&
    verification.isError &&
    (verification.error as { status?: number } | null)?.status !== 404;
  const showVerified =
    verified != null || verificationPending || verificationFailed;
  const effectiveKey =
    (activeKey === 'code' && !hasWasm) ||
    (activeKey === 'verified' && !showVerified)
      ? 'interface'
      : activeKey;

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
                Decompilation
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
    ...(showVerified
      ? [
          {
            key: 'verified',
            // Emerald dot = verified, the counterpart of the amber
            // "experimental" dot on Decompilation.
            label: (
              <Box
                component="span"
                sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.75 }}
              >
                Verified Code
                <Box
                  component="span"
                  sx={{
                    width: 6,
                    height: 6,
                    borderRadius: '50%',
                    bgcolor: 'emerald.400',
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
          {/* Task 0472: the header names what this contract IS — for every
              class, not just SAC — and links to it. See `contractFace` /
              `ContractFaceChip` (shared with the list row). */}
          {face != null && <ContractFaceChip face={face} size="md" />}
          {/* Task 0327 — mutability, 3-state; null/undefined (Unknown) → no chip.
              Label states exactly what the WASM import scan proves ("self-
              upgrade path present/absent"), not the broader "immutable" — a
              static scan can't see proxy/delegate or renounced-admin patterns. */}
          {/* Task 0548 — CAP-85: the code is set by another contract, so the
              self-upgrade scan does not apply (upgradeable is null here). */}
          {contract.data?.executable_owner && (
            <Tooltip
              title="Code is set by another contract. Changing it there changes every contract that uses the same tag."
              describeChild
            >
              <span>
                <Chip size="md" color="neutral" label="Externally managed" />
              </span>
            </Tooltip>
          )}
          {verified != null && (
            // Task 0639: a third party's claim, read from BlockSmith — the
            // tooltip names who made it, as on "Externally managed". A plain
            // label, like its neighbours: the tab below is opened from the tabs.
            <Tooltip
              title={
                <>
                  Rebuilt from its source by:
                  {verifiedByNames(verified).map((n, i) => (
                    <bdi key={i} style={{ display: 'block' }}>
                      {n}
                    </bdi>
                  ))}
                  Not an audit.
                </>
              }
              describeChild
            >
              <span>
                <Chip size="md" color="emerald" label="Verified source" />
              </span>
            </Tooltip>
          )}
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
          {/* Keyed by contract AND tab: a crash on one contract must not
              survive navigation to another (the boundary would otherwise
              keep showing its error state on healthy data). */}
          <SectionErrorBoundary
            key={`${contractId}-${effectiveKey}`}
            sectionName={`contract-${effectiveKey}`}
          >
            {effectiveKey === 'interface' && (
              <ContractInterface contractId={contractId} />
            )}
            {effectiveKey === 'code' && contract.data?.wasm_hash != null && (
              // Keyed by contract: navigating between contracts must not
              // carry over the "Rust unavailable" state of the previous one.
              <ContractCode
                key={contractId}
                contractId={contractId}
                wasmHash={contract.data.wasm_hash}
              />
            )}
            {effectiveKey === 'verified' &&
              (verified != null ? (
                <VerifiedCode
                  key={contractId}
                  contractId={contractId}
                  verification={verified}
                />
              ) : verificationFailed ? (
                <Stack spacing={1.5} sx={{ p: 2 }}>
                  <QueryErrorState
                    error={verification.error}
                    onRetry={() => void verification.refetch()}
                  />
                </Stack>
              ) : (
                <Stack spacing={1.5} sx={{ p: 2 }}>
                  <CardSkeleton />
                </Stack>
              ))}
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
