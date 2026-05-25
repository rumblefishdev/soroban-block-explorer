import type { TransactionListItem } from '@rumblefish/api-types';
import {
  PaginationControls,
  TableEmptyState,
  TableSectionHeader,
} from '@rumblefish/soroban-block-explorer-ui';
import { Box, Card } from '@mui/material';

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
      <Box sx={{ minHeight: 200 }}>
        {rows.length === 0 ? (
          <Box sx={{ display: 'flex', justifyContent: 'center', py: 8 }}>
            <TableEmptyState
              kind="transactions"
              title="No transactions in this ledger"
              description="This ledger closed without any transactions."
            />
          </Box>
        ) : (
          <TransactionsTable rows={rows} />
        )}
      </Box>
      <PaginationControls
        caption={`${totalCount.toLocaleString('en-US')} transactions`}
        canPrev={canPrev}
        canNext={canNext}
        onPrev={onPrev}
        onNext={onNext}
      />
    </Card>
  );
}
