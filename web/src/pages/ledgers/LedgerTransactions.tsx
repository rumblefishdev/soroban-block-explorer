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
        // not the fetched page — decides whether "closed without any" is a
        // fact. A header indexed ahead of its transactions has a non-zero
        // count and no rows; claiming an empty ledger there is wrong (0377 F7).
        <TableEmptyState
          kind="transactions"
          title={
            totalCount === 0
              ? 'No transactions in this ledger'
              : 'Transactions unavailable'
          }
          description={
            totalCount === 0
              ? 'This ledger closed without any transactions.'
              : `This ledger closed with ${formatInteger(
                  totalCount
                )} transactions, but none could be loaded.`
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
