import {
  listPoolsInfiniteOptions,
  type ListPoolsData,
} from '@rumblefish/api-types';
import { useInfiniteQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListPoolsData['query']>;

/**
 * Fetches the paginated liquidity-pool list (`GET /liquidity-pools`).
 *
 * Supported filters per task 0246:
 *   * `filter[asset_code]` — single-asset, case-insensitive, matches either
 *     leg (preferred for the Figma "Filter by asset pair" input).
 *   * `filter[min_tvl]` — decimal threshold.
 *   * Per-leg `filter[asset_a_code/issuer]` / `filter[asset_b_code/issuer]`
 *     remain available for API consumers needing issuer disambiguation.
 *
 * Cursor pagination via `lastPage.page.cursor`.
 */
export const usePoolsList = (filters?: Filters) =>
  useInfiniteQuery({
    ...listPoolsInfiniteOptions(filters ? { query: filters } : undefined),
    ...listPolicy,
    initialPageParam: {},
    getNextPageParam: (lastPage) => lastPage.page.cursor ?? undefined,
  });
