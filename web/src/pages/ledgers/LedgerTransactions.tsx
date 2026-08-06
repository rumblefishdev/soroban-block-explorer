import type { TransactionListItem } from '@rumblefish/api-types';
import {
  formatInteger,
  PaginationControls,
  TableEmptyState,
  TableSectionHeader,
} from '@rumblefish/soroban-block-explorer-ui';
import { Card } from '@mui/material';

import { TransactionsTable } from '../transactions/TransactionsTable.js';

interface LedgerTransactionsProps {
  rows: readonly TransactionListItem[];
  totalCount: number;
  canPrev: boolean;
  canNext: boolean;
  onPrev: () => void;
  onNext: () => void;
}

/** The "transactions in this ledger" section of the ledger detail page. */
export function LedgerTransactions({
  rows,
  totalCount,
  canPrev,
  canNext,
  onPrev,
  onNext,
}: LedgerTransactionsProps) {
  return (
    <Card>
      <TableSectionHeader title="Transactions in this ledger" />
      {rows.length === 0 ? (
        // `totalCount` is the ledger header's own transaction_count, so it —
        // not the fetched page — decides what an empty page means. The header
        // is written before its transactions (two-phase ingest), so a recent
        // ledger legitimately reports a count with no rows yet: that is
        // "not indexed yet", NOT a failure, and it is the common case on the
        // head ledger linked from the home page (0377 F7).
        <TableEmptyState
          kind="transactions"
          variant={totalCount === 0 ? 'default' : 'warning'}
          title={
            totalCount === 0
              ? 'No transactions in this ledger'
              : 'Not indexed yet'
          }
          description={
            totalCount === 0
              ? 'This ledger closed without any transactions.'
              : 'Its transactions are still being indexed — try again shortly.'
          }
        />
      ) : (
        <TransactionsTable rows={rows} />
      )}
      <PaginationControls
        caption={`${formatInteger(totalCount)} transactions`}
        canPrev={canPrev}
        canNext={canNext}
        onPrev={onPrev}
        onNext={onNext}
      />
    </Card>
  );
}
