import SwapHorizIcon from '@mui/icons-material/SwapHorizOutlined';
import { Card } from '@mui/material';
import type { NftTransferItem } from '@rumblefish/api-types';
import {
  Dash,
  EmptyState,
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  IdentifierDisplay,
  TableSectionHeader,
  useCursorPagination,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { useNftTransfers, usePagedRows } from '../../api/index.js';
import { DataList } from '../detail/DataList.js';
import { TransactionTime } from '../transactions/TransactionTime.js';

import { NftEventBadge } from './NftEventBadge.js';

interface NftTransfersProps {
  /** Issuing contract C-strkey. */
  contractId: string;
  /** Opaque contract-defined token id (≤128 ASCII). */
  tokenId: string;
}

const columns: ExplorerTableColumn<NftTransferItem>[] = [
  {
    id: 'event',
    header: 'Event',
    width: 120,
    cell: (row) => <NftEventBadge name={row.event_type_name} />,
  },
  {
    id: 'from',
    header: 'From',
    width: 160,
    // `from_account` is null on the mint row.
    cell: (row) =>
      row.from_account ? (
        <IdentifierDisplay value={row.from_account} type="account" />
      ) : (
        <Dash />
      ),
  },
  {
    id: 'to',
    header: 'To',
    width: 160,
    // `to_account` is null on a burn.
    cell: (row) =>
      row.to_account ? (
        <IdentifierDisplay value={row.to_account} type="account" />
      ) : (
        <Dash />
      ),
  },
  {
    id: 'transaction',
    header: 'Transaction',
    width: 160,
    cell: (row) => (
      <IdentifierDisplay value={row.transaction_hash} type="transaction" />
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
 * Transfer-history section of the NFT detail page — a cursor-paginated table
 * of mint / transfer / burn events for one NFT, per the Figma design.
 */
export function NftTransfers({ contractId, tokenId }: NftTransfersProps) {
  // Cursors are NFT-scoped — drop the URL cursor on NFT switch.
  const { cursor, goNext, goPrev } = useCursorPagination({
    resetKey: `${contractId}/${tokenId}`,
  });

  const query = useNftTransfers(contractId, tokenId, cursor);
  const pager = usePagedRows(query.data, goNext, goPrev);

  return (
    <Card>
      <TableSectionHeader title="Transfer history" />
      <DataList
        query={query}
        pager={pager}
        renderTable={(rows, { loading }) => (
          <ExplorerTable
            columns={columns}
            rows={rows}
            rowKey={(row) => `${row.transaction_hash}-${row.event_order}`}
            loading={loading}
            skeletonRows={20}
            rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
          />
        )}
        renderEmpty={() => (
          <EmptyState
            icon={<SwapHorizIcon />}
            title="No transfer history"
            description="This NFT has no recorded mint, transfer or burn events."
            py={8}
          />
        )}
      />
    </Card>
  );
}
