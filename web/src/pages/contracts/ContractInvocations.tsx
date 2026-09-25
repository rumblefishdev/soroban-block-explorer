import { Box } from '@mui/material';
import type { PaginatedInvocationItem } from '@rumblefish/api-types';
import {
  Dash,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  IdentifierDisplay,
  IdentifierWithCopy,
  TableEmptyState,
  useCursorPagination,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { useContractInvocations, usePagedRows } from '../../api/index.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { DataListCard } from '../detail/DataListCard.js';
import { ledgerColumn, statusColumn } from '../transactions/cells.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

type InvocationRow = PaginatedInvocationItem['data'][number];

const rowKey = (row: InvocationRow, index: number) =>
  `${row.transaction_hash}-${row.ledger_sequence}-${index}`;

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
  statusColumn<InvocationRow>(),
  ledgerColumn<InvocationRow>(),
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

  const { rows, canPrev, canNext, handlePrev, handleNext } = usePagedRows(
    data,
    goNext,
    goPrev
  );

  return (
    <DataListCard
      // Bare, inside the contract page's tab card — no card of its own.
      renderContainer={(content) => <Box>{content}</Box>}
      columnCount={columns.length}
      isLoading={isLoading}
      isReloading={isPlaceholderData}
      isError={isError}
      error={error}
      onRetry={() => void refetch()}
      errorPy={6}
      rows={rows}
      renderSkeleton={() => (
        <ExplorerTable
          columns={columns}
          rows={[]}
          rowKey={rowKey}
          loading
          skeletonRows={20}
          rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
        />
      )}
      renderTable={(pageRows) => (
        <ExplorerTable
          columns={columns}
          rows={pageRows}
          rowKey={rowKey}
          rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
        />
      )}
      renderEmpty={() => (
        <TableEmptyState
          kind="transactions"
          title="No invocations"
          description="This contract has not been invoked yet."
        />
      )}
      emptyNoun="invocations"
      canPrev={canPrev}
      canNext={canNext}
      onPrev={handlePrev}
      onNext={handleNext}
    />
  );
}
