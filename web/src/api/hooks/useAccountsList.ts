import { useQuery } from '@tanstack/react-query';

import { listPolicy } from '../polling.js';

export type AccountsSort = 'xlm_desc' | 'last_seen_desc' | 'first_seen_desc';

export interface AccountListItem {
  account_id: string;
  xlm_balance: string;
  xlm_supply_percent: number;
  first_seen_ledger: number;
  last_seen_ledger: number;
  home_domain: string | null;
}

export interface AccountsListResponse {
  data: AccountListItem[];
  page: {
    limit: number;
    next_cursor: string | null;
    prev_cursor: string | null;
  };
}

export interface AccountsListFilters {
  q?: string;
  with_domain?: boolean;
  sort?: AccountsSort;
  limit?: number;
}

// Local fixtures so the page can be design-checked without the backend
// `GET /v1/accounts` endpoint (not yet implemented). Replace this whole
// module with a generated `listAccountsOptions` hook when the API lands.
const DOMAIN_POOL = [
  'stellar.org',
  'satoshipay.io',
  'lobstr.co',
  'mintbase.com',
  'anchorusd.com',
  'sdex.io',
  'fairx.io',
  'ultrastellar.com',
  'kraken.com',
  'binance.com',
  'coinbase.com',
  'bitstamp.net',
];
const BASE32 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
function synthAccountId(seed: number): string {
  let s = (seed * 9973 + 12345) >>> 0;
  let body = '';
  for (let n = 0; n < 55; n++) {
    s = (Math.imul(s, 1103515245) + 12345) >>> 0;
    body += BASE32[(s >>> 8) % 32];
  }
  return `G${body}`;
}
const TOTAL_XLM_SUPPLY = 50_001_812_343;
function buildRow(i: number): AccountListItem {
  const xlmWhole = Math.max(
    10_000,
    Math.round(4_107_709_533 * Math.pow(0.92, i))
  );
  const last_seen_ledger =
    i % 5 === 4
      ? 50_000_000 - ((i * 31_181) % 5_000_000)
      : 54_837_201 - ((i * 911) % 80_000);

  const age = 100_000 + ((i * 4_999) % 30_000_000);
  const first_seen_ledger = Math.max(1_000_000, last_seen_ledger - age);
  return {
    account_id: synthAccountId(i + 1),
    xlm_balance: `${xlmWhole}.0000000`,
    xlm_supply_percent: Number(
      ((xlmWhole / TOTAL_XLM_SUPPLY) * 100).toFixed(4)
    ),
    first_seen_ledger,
    last_seen_ledger,
    home_domain: i % 3 === 0 ? DOMAIN_POOL[i % DOMAIN_POOL.length] : null,
  };
}
const ACCOUNTS: AccountListItem[] = Array.from({ length: 80 }, (_, i) =>
  buildRow(i)
);

function sliceMock(
  cursor: string | null,
  filters: AccountsListFilters
): AccountsListResponse {
  const limit = filters.limit ?? 20;
  const offset = cursor ? Math.max(0, Number(cursor) || 0) : 0;
  const q = (filters.q ?? '').trim().toUpperCase();
  const sort = filters.sort ?? 'xlm_desc';

  let rows = ACCOUNTS;
  if (q) rows = rows.filter((a) => a.account_id.includes(q));
  if (filters.with_domain) rows = rows.filter((a) => a.home_domain != null);
  const sorted = [...rows].sort((a, b) => {
    if (sort === 'last_seen_desc')
      return b.last_seen_ledger - a.last_seen_ledger;
    if (sort === 'first_seen_desc')
      return b.first_seen_ledger - a.first_seen_ledger;
    return Number(b.xlm_balance) - Number(a.xlm_balance);
  });
  const total = sorted.length;
  const start = Math.min(offset, total);
  const end = Math.min(start + limit, total);
  return {
    data: sorted.slice(start, end),
    page: {
      limit,
      next_cursor: end < total ? String(end) : null,
      prev_cursor: start > 0 ? String(Math.max(0, start - limit)) : null,
    },
  };
}

export const useAccountsList = (
  cursor: string | null,
  filters: AccountsListFilters
) =>
  useQuery({
    queryKey: ['accounts-list', cursor, filters],
    queryFn: () => Promise.resolve(sliceMock(cursor, filters)),
    ...listPolicy,
  });
