import { useCursorPagination } from '@rumblefish/soroban-block-explorer-ui';

import { PAGE_SIZE, useNftsList, usePagedRows } from '../../api/index.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { DataList } from '../detail/DataList.js';
import { NftsTable } from '../nfts/NftsTable.js';

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
        <NftsTable
          rows={rows}
          loading={loading}
          skeletonRows={PAGE_SIZE}
          hideOwner
        />
      )}
      emptyKind="nft"
      emptyPy={6}
    />
  );
}
