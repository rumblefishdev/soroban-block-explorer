import { screen } from '@testing-library/react';
import { Route, Routes } from 'react-router-dom';
import { describe, expect, it, vi } from 'vitest';

import type { SearchResultsState } from '../search/useSearchResults.js';
import { renderWithProviders } from '../test-utils.js';
import SearchResultsPage from './SearchResultsPage.js';

const HIT_ACCOUNT = 'GA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVSGZ';

/** One account hit, nothing else — the case 0271 used to redirect on. */
const ONE_HIT: readonly SearchResultsState['hitsForActiveTab'][number][] = [
  {
    entity_type: 'account',
    identifier: HIT_ACCOUNT,
    label: 'GA7Q…VSGZ',
  } as SearchResultsState['hitsForActiveTab'][number],
];

vi.mock('../search/useSearchResults.js', async (importOriginal) => {
  const actual = await importOriginal<
    typeof import('../search/useSearchResults.js')
  >();
  const base = {
    data: undefined,
    isFetching: false,
    isError: false,
    error: null,
    refetch: () => undefined,
    setActiveTab: () => undefined,
    counts: {
      transaction: 0,
      account: 0,
      contract: 0,
      asset: 0,
      nft: 0,
      pool: 0,
    },
    totalCount: 0,
    activeTab: 'transaction' as const,
    hitsForActiveTab: [],
  };
  return {
    ...actual,
    useSearchResults: vi.fn(
      (params: { q: string }): SearchResultsState =>
        params.q === 'one-hit'
          ? ({
              ...base,
              effectiveQuery: params.q,
              // `SearchResultsView` renders rows only when `data` is present.
              data: {
                groups: { accounts: ONE_HIT },
              } as SearchResultsState['data'],
              counts: { ...base.counts, account: 1 },
              totalCount: 1,
              activeTab: 'account',
              hitsForActiveTab: ONE_HIT,
            } as SearchResultsState)
          : ({ ...base, effectiveQuery: params.q } as SearchResultsState)
    ),
  };
});

/** `/search` plus a stand-in account page, so a redirect would be visible. */
function renderSearch(q: string) {
  return renderWithProviders(
    <Routes>
      <Route path="/search" element={<SearchResultsPage />} />
      <Route path="/accounts/:accountId" element={<div>account page</div>} />
    </Routes>,
    { initialEntries: [`/search?q=${encodeURIComponent(q)}`] }
  );
}

describe('SearchResultsPage — single hit (task 0527)', () => {
  // 0271 navigated away here. It took the page before the match could be
  // read, and `replace: true` meant Back could not bring it back.
  it('shows the one result instead of navigating to it', async () => {
    renderSearch('one-hit');

    const links = await screen.findAllByRole('link');
    expect(links.map((a) => a.getAttribute('href'))).toContain(
      `/accounts/${HIT_ACCOUNT}`
    );
    expect(screen.queryByText('account page')).not.toBeInTheDocument();
  });
});
