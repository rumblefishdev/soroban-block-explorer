import { Box } from '@mui/material';
import type { PaginatedInvocationItem } from '@rumblefish/api-types';
import {
  Dash,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  IdentifierDisplay,
  IdentifierWithCopy,
  PaginationControls,
  QueryErrorState,
  StatusChip,
  TableEmptyState,
  useCursorPagination,
  usePageHandlers,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { useContractInvocations } from '../../api/index.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

type InvocationRow = PaginatedInvocationItem['data'][number];

// Figma shows a "Function" column, but the invocations appearance index
// carries no per-call function name (ADR 0034 — call detail is XDR-only).
// The transaction hash takes its place: it links to the full call detail.
const columns: ExplorerTableColumn<InvocationRow>[] = [
  {
    id: 'transaction',
    header: 'Transaction',
    width: 160,
    cell: (row) => (
      <IdentifierWithCopy value={row.transaction_hash} type="transaction" />
    ),
  },
  {
    id: 'caller',
    header: 'Caller',
    width: 160,
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
    width: 120,
    cell: (row) => <StatusChip successful={row.successful} />,
  },
  {
    id: 'ledger',
    header: 'Ledger',
    width: 120,
    cell: (row) => (
      <IdentifierDisplay value={String(row.ledger_sequence)} type="ledger" />
    ),
  },
  {
    id: 'time',
    header: 'Time',
    width: 210,
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

  const { data, isLoading, isPlaceholderData, isError, error, refetch } =
    useContractInvocations(contractId, cursor);

  const rows = data?.data ?? [];
  const { canPrev, canNext, handlePrev, handleNext } = usePageHandlers(
    data?.page,
    goNext,
    goPrev
  );

  let body: ReactNode;
  if (isLoading || isPlaceholderData) {
    body = (
      <ExplorerTable
        columns={columns}
        rows={[]}
        rowKey={(row, index) =>
          `${row.transaction_hash}-${row.ledger_sequence}-${index}`
        }
        loading
        skeletonRows={20}
        rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      />
    );
  } else if (isError) {
    body = <QueryErrorState error={error} onRetry={() => void refetch()} />;
  } else if (rows.length === 0) {
    body = (
      <TableEmptyState
        kind="transactions"
        title="No invocations"
        description="This contract has not been invoked yet."
      />
    );
  } else {
    body = (
      <ExplorerTable
        columns={columns}
        rows={rows}
        rowKey={(row, index) =>
          `${row.transaction_hash}-${row.ledger_sequence}-${index}`
        }
        rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
      />
    );
  }

  return (
    <Box>
      {body}
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
