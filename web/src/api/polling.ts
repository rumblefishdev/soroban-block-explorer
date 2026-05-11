export const homePolicy = {
  staleTime: 10_000,
  refetchInterval: 12_000,
} as const;

export const listPolicy = {
  staleTime: 60_000,
} as const;

export const detailPolicy = {
  staleTime: 5 * 60_000,
} as const;

export const searchPolicy = {
  staleTime: 0,
  gcTime: 0,
} as const;
