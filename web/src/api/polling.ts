import { keepPreviousData } from '@tanstack/react-query';

export const homePolicy = {
  staleTime: 10_000,
  refetchInterval: 12_000,
} as const;

/**
 * Default policy for cursor-paginated list queries. `placeholderData:
 * keepPreviousData` keeps the previous page's rows on screen while the
 * next cursor is being fetched, so Next clicks don't flash a spinner
 * between pages.
 */
export const listPolicy = {
  staleTime: 60_000,
  placeholderData: keepPreviousData,
} as const;

/**
 * Default policy for detail queries with embedded cursor-paginated
 * sub-sections (e.g. ledger transactions). Same `placeholderData` rule
 * as `listPolicy` for the embedded list.
 */
export const detailPolicy = {
  staleTime: 5 * 60_000,
  placeholderData: keepPreviousData,
} as const;

export const searchPolicy = {
  staleTime: 0,
  gcTime: 0,
} as const;
