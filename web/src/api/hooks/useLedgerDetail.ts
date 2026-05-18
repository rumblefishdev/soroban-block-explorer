import { getLedgerInfiniteOptions } from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * `GET /ledgers/:sequence` — ledger header plus its embedded, cursor-
 * paginated transactions. Each page repeats the header and carries one
 * page of transactions; the detail page slices pages by index.
 */
export const useLedgerDetail = (sequence: number, enabled = true) =>
  useInfiniteQuery({
    ...getLedgerInfiniteOptions({ path: { sequence } }),
    ...detailPolicy,
    enabled: enabled && sequence > 0,
    initialPageParam: { path: { sequence } },
    getNextPageParam: (lastPage) =>
      lastPage.transactions.page.cursor ?? undefined,
  });
