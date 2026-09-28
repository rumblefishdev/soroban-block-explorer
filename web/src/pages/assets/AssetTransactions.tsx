import type { AssetTransactionItem } from '@rumblefish/api-types';
import {
  Dash,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  IdentifierDisplay,
  IdentifierWithCopy,
  StatusChip,
  useCursorPagination,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { useAssetTransactions, usePagedRows } from '../../api/index.js';
import { DataList } from '../detail/DataList.js';
import { SectionCard } from '../detail/SectionCard.js';
import { OperationCell } from '../transactions/cells.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

const columns: ExplorerTableColumn<AssetTransactionItem>[] = [
  {
    id: 'hash',
    header: 'Hash',
    width: 160,
    cell: (row) => <IdentifierWithCopy value={row.hash} type="transaction" />,
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
    id: 'source',
    header: 'Source account',
    width: 160,
    cell: (row) =>
      row.source_account ? (
        <IdentifierDisplay value={row.source_account} type="account" />
      ) : (
        <Dash />
      ),
  },
  {
    id: 'operation',
    header: 'Operation',
    width: 190,
    cell: (row) => <OperationCell types={row.operation_types} />,
  },
  {
    id: 'status',
    header: 'Status',
    width: 120,
    cell: (row) => <StatusChip successful={row.successful} />,
  },
  {
    id: 'time',
    header: 'Time',
    width: 210,
    cell: (row) => <TransactionTime createdAt={row.created_at} />,
  },
];

/**
 * Latest transactions section of the asset detail page — a paginated table of
 * transactions involving the asset, fetched independently of the asset
 * summary and metadata.
 */
export function AssetTransactions({ assetId }: { assetId: string }) {
  // Cursors are asset-scoped — drop the URL cursor on asset switch.
  const { cursor, goNext, goPrev } = useCursorPagination({
    resetKey: assetId,
  });

  const query = useAssetTransactions(assetId, cursor);
  const pager = usePagedRows(query.data, goNext, goPrev);

  return (
    <SectionCard title="Latest transactions">
      <DataList
        query={query}
        pager={pager}
        errorPy={6}
        renderTable={(rows, { loading }) => (
          <ExplorerTable
            columns={columns}
            rows={rows}
            rowKey={(row) => row.hash}
            loading={loading}
            skeletonRows={20}
            rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
          />
        )}
        emptyKind="transactions"
        emptyPy={6}
      />
    </SectionCard>
  );
}
