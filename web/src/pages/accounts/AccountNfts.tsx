import { useCursorPagination } from '@rumblefish/soroban-block-explorer-ui';

import { PAGE_SIZE, useNftsList, usePagedRows } from '../../api/index.js';
import { CURSOR_PARAMS } from '../cursorParams.js';
import { DataList } from '../detail/DataList.js';
import { SectionCard } from '../detail/SectionCard.js';
import { NftsTable } from '../nfts/NftsTable.js';

/**
 * NFTs section of the account detail page — the NFTs the account holds now,
 * each linked to its NFT page. Few accounts hold any (856 of ~20M on
 * mainnet, task 0675), so the section shows only once its first page comes
 * back with rows; a failed fetch still shows, so the error is not hidden.
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

  if (query.isLoading) return null;
  if (!query.isError && cursor == null && pager.rows.length === 0) return null;

  return (
    <SectionCard title="NFTs">
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
    </SectionCard>
  );
}
