import { Box } from '@mui/material';
import type { PaginatedInvocationItem } from '@rumblefish/api-types';
import {
  classifyError,
  ExplorerTable,
  GenericErrorState,
  IdentifierDisplay,
  IdentifierWithCopy,
  PaginationControls,
  RateLimitState,
  TableEmptyState,
  TableSkeleton,
  TransientErrorState,
  useCursorPagination,
  usePageHandlers,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { useContractInvocations } from '../../api/index.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { Dash, StatusCell } from '../transactions/cells.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

type InvocationRow = PaginatedInvocationItem['data'][number];

// Figma shows a "Function" column, but the invocations appearance index
// carries no per-call function name (ADR 0034 — call detail is XDR-only).
// The transaction hash takes its place: it links to the full call detail.
const columns: ExplorerTableColumn<InvocationRow>[] = [
  {
    id: 'transaction',
    header: 'Transaction',
    cell: (row) => (
      <IdentifierWithCopy value={row.transaction_hash} type="transaction" />
    ),
  },
  {
    id: 'caller',
    header: 'Caller',
    cell: (row) =>
      row.caller_account ? (
        <IdentifierDisplay value={row.caller_account} type="account" />
      ) : (
        <Dash />
      ),
  },
  {
    id: 'status',
    header: 'Status',
    cell: (row) => <StatusCell successful={row.successful} />,
  },
  {
    id: 'ledger',
    header: 'Ledger',
    cell: (row) => (
      <IdentifierDisplay value={String(row.ledger_sequence)} type="ledger" />
    ),
  },
  {
    id: 'time',
    header: 'Time',
    cell: (row) => <TransactionTime createdAt={row.created_at} />,
  },
];

/**
 * Invocations tab — a paginated table of the contract's invocation
 * appearances. Fetched independently of the summary and interface so a
 * failure here never collapses the rest of the page.
 */
export function ContractInvocations({ contractId }: { contractId: string }) {
  // Namespaced cursor: contract detail tabs between Events + Invocations.
  // `resetKey` drops the cursor when the user navigates to a different
  // contract.
  const { cursor, goNext, goPrev } = useCursorPagination({
    cursorParam: CURSOR_PARAMS.CONTRACT_INVOCATIONS,
    resetKey: contractId,
  });

  const { data, isLoading, isError, error, refetch } = useContractInvocations(
    contractId,
    cursor
  );

  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );

  let body: ReactNode;
  if (isLoading) {
    body = (
      <Box sx={{ p: 2 }}>
        <TableSkeleton rows={8} columns={columns.length} />
      </Box>
    );
  } else if (isError) {
    const kind = classifyError(error);
    const retry = () => void refetch();
    body = (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 6 }}>
        {kind === 'rate-limit' ? (
          <RateLimitState onRetry={retry} />
        ) : kind === 'transient' ? (
          <TransientErrorState onRetry={retry} />
        ) : (
          <GenericErrorState onRetry={retry} />
        )}
      </Box>
    );
  } else if (rows.length === 0) {
    body = (
      <Box sx={{ display: 'flex', justifyContent: 'center', py: 6 }}>
        <TableEmptyState
          kind="transactions"
          title="No invocations"
          description="This contract has not been invoked yet."
        />
      </Box>
    );
  } else {
    body = (
      <ExplorerTable
        columns={columns}
        rows={rows}
        rowKey={(row, index) =>
          `${row.transaction_hash}-${row.ledger_sequence}-${index}`
        }
      />
    );
  }

  return (
    <Box>
      <Box sx={{ minHeight: 280 }}>{body}</Box>
      <PaginationControls
        caption="Latest results"
        canPrev={canPrev}
        canNext={canNext}
        onPrev={handlePrev}
        onNext={handleNext}
      />
    </Box>
  );
}
