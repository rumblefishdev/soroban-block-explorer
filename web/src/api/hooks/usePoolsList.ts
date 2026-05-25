import { listPoolsOptions, type ListPoolsData } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

type Filters = NonNullable<ListPoolsData['query']>;

/**
 * `GET /liquidity-pools` — cursor-paginated liquidity-pool list.
 *
 * Supported filters per task 0246:
 *   * `filter[asset_code]` — single-asset, case-insensitive, matches either
 *     leg (preferred for the Figma "Filter by asset pair" input).
 *   * `filter[min_tvl]` — decimal threshold.
 *   * Per-leg `filter[asset_a_code/issuer]` / `filter[asset_b_code/issuer]`
 *     remain available for API consumers needing issuer disambiguation.
 *
 * Each (filter, cursor) combination forms a distinct queryKey, so
 * revisiting a cursor is a cache hit. URL-as-state pagination — caller
 * passes the current cursor from `useCursorPagination`.
 */
export const usePoolsList = (cursor: string | null = null, filters?: Filters) =>
  useQuery({
    ...listPoolsOptions({
      query: { ...(filters ?? {}), ...(cursor ? { cursor } : {}) },
    }),
    ...listPolicy,
  });
