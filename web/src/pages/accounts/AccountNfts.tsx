import type { NftItem } from '@rumblefish/api-types';
import {
  EXPLORER_TABLE_ROW_HEIGHT_TALL,
  ExplorerTable,
  IdentifierDisplay,
  useCursorPagination,
  type ExplorerTableColumn,
} from '@rumblefish/soroban-block-explorer-ui';

import { PAGE_SIZE, useNftsList, usePagedRows } from '../../api/index.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { DataList } from '../detail/DataList.js';
import { NftNameCell } from '../nfts/NftNameCell.js';

// The `/nfts` list's columns without Owner: on the account's own page every
// owner is that account.
const columns: ExplorerTableColumn<NftItem>[] = [
  {
    id: 'nft',
    header: 'NFT',
    width: 240,
    cell: (row) => <NftNameCell row={row} />,
  },
  {
    id: 'contract',
    header: 'Contract ID',
    width: 160,
    cell: (row) => (
      <IdentifierDisplay value={row.contract_id} type="contract" />
    ),
  },
];

/**
 * The NFTs tab of the account's Assets card — the NFTs the account holds now,
 * each linked to its NFT page, newest mint first.
 */
export function AccountNfts({ accountId }: { accountId: string }) {
  // Own URL key: the transactions section below pages with `?cursor=`.
  const { cursor, goNext, goPrev } = useCursorPagination({
    cursorParam: CURSOR_PARAMS.ACCOUNT_NFTS,
    resetKey: accountId,
  });
  const query = useNftsList(cursor, {
    limit: PAGE_SIZE,
    'filter[owner]': accountId,
  });
  const pager = usePagedRows(query.data, goNext, goPrev);

  return (
    <DataList
      query={query}
      pager={pager}
      errorPy={6}
      renderTable={(rows, { loading }) => (
        // No header row: the Tokens list beside it in this card has none.
        <ExplorerTable
          columns={columns}
          rows={rows}
          rowKey={(row) => `${row.contract_id}:${row.token_id}`}
          rowHeight={EXPLORER_TABLE_ROW_HEIGHT_TALL}
          loading={loading}
          skeletonRows={PAGE_SIZE}
          showHeader={false}
        />
      )}
      emptyKind="nft"
      emptyPy={6}
    />
  );
}
