import { getLedgerOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * `GET /ledgers/:sequence` — ledger header plus embedded, cursor-
 * paginated transactions. Each `cursor` is a distinct queryKey, so the
 * transactions table pages instantly when revisiting a cached cursor.
 * The header is duplicated across pages (backend echoes it), which is
 * fine — it's small and identical, RQ cache absorbs it.
 */
export const useLedgerDetail = (
  sequence: number,
  cursor: string | null = null,
  enabled = true
) =>
  useQuery({
    ...getLedgerOptions({
      path: { sequence },
      query: cursor ? { cursor } : undefined,
    }),
    ...detailPolicy,
    enabled: enabled && sequence > 0,
  });
