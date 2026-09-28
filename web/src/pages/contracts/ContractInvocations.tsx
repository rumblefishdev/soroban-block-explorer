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
import { DataList } from '../detail/DataList.js';
import { ledgerColumn, statusColumn } from '../transactions/cells.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

type InvocationRow = PaginatedInvocationItem['data'][number];

const rowKey = (row: InvocationRow, index: number) =>
  `${row.transaction_hash}-${row.ledger_sequence}-${index}`;

/**
 * The invocation's caller: an account or a contract (task 0487 — a contract
 * caller used to render as a dash). The dash is left for a row with neither.
 */
export function CallerCell({
  row,
}: {
  row: Pick<InvocationRow, 'caller_account' | 'caller_contract'>;
}) {
  if (row.caller_account) {
    return <IdentifierDisplay value={row.caller_account} type="account" />;
  }
  if (row.caller_contract) {
    return <IdentifierDisplay value={row.caller_contract} type="contract" />;
  }
  return <Dash />;
}

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
    cell: (row) => <CallerCell row={row} />,
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

  const query = useContractInvocations(contractId, cursor);
  const pager = usePagedRows(query.data, goNext, goPrev);

  return (
    // Bare, inside the contract page's tab card — no frame of its own.
    <DataList
      query={query}
      pager={pager}
      errorPy={6}
      renderTable={(rows, { loading }) => (
        <ExplorerTable
          columns={columns}
          rows={rows}
          rowKey={rowKey}
          loading={loading}
          skeletonRows={20}
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
    />
  );
}
