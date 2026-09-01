import { screen, waitFor } from '@testing-library/react';
import { Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../test-utils.js';
import type { SearchResultsState } from '../search/useSearchResults.js';
import SearchResultsPage from './SearchResultsPage.js';

/** One account hit, nothing else — the case 0271 used to redirect on. */
const ONE_HIT_STATE: Partial<SearchResultsState> = {
  counts: {
    transaction: 0,
    account: 1,
    contract: 0,
    asset: 0,
    nft: 0,
    pool: 0,
  },
  totalCount: 1,
  activeTab: 'account',
  hitsForActiveTab: [
    {
      entity_type: 'account',
      identifier: 'GA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVSGZ',
      label: 'GA7Q…VSGZ',
    } as SearchResultsState['hitsForActiveTab'][number],
  ],
};

vi.mock('../search/useSearchResults.js', async (importOriginal) => {
  const actual = await importOriginal<
    typeof import('../search/useSearchResults.js')
  >();
  return {
    ...actual,
    useSearchResults: vi.fn(
      (params: { q: string }): SearchResultsState =>
        params.q === 'one-hit'
          ? ({
              ...ONE_HIT_STATE,
              effectiveQuery: params.q,
              // `SearchResultsView` renders rows only when `data` is present.
              data: {
                groups: { accounts: ONE_HIT_STATE.hitsForActiveTab },
              } as SearchResultsState['data'],
              isFetching: false,
              isError: false,
              error: null,
              refetch: () => undefined,
              setActiveTab: () => undefined,
            } as SearchResultsState)
          : ({
              effectiveQuery: params.q,
              data: undefined,
              isFetching: false,
              isError: false,
              error: null,
              refetch: () => undefined,
              counts: {
                transaction: 0,
                account: 0,
                contract: 0,
                asset: 0,
                nft: 0,
                pool: 0,
              },
              totalCount: 0,
              activeTab: 'transaction',
              setActiveTab: () => undefined,
              hitsForActiveTab: [],
            } as SearchResultsState)
    ),
  };
});

const ACCOUNT = 'GC526FUILJ6NLFXKCOOGTMDXNRW7MYSEK2UNRJV5FYWOGYDE4LOKXFEM';
const TOML = 'https://lobstr.co/.well-known/stellar.toml';
const FEDERATION = 'https://lobstr.co/federation/';

function reply(body: string) {
  return { ok: true, status: 200, text: () => Promise.resolve(body) };
}

function mockFetch(routes: Record<string, ReturnType<typeof reply>>) {
  vi.stubGlobal(
    'fetch',
    vi.fn((url: string) => {
      const hit = Object.entries(routes).find(([p]) => url.startsWith(p));
      return hit
        ? Promise.resolve(hit[1])
        : Promise.reject(new Error('ENOTFOUND'));
    })
  );
}

/** `/search` plus a stand-in account page, so a redirect is observable. */
function renderSearch(q: string) {
  return renderWithProviders(
    <Routes>
      <Route path="/search" element={<SearchResultsPage />} />
      <Route path="/accounts/:accountId" element={<div>account page</div>} />
    </Routes>,
    { initialEntries: [`/search?q=${encodeURIComponent(q)}`] }
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('SearchResultsPage — federated addresses (task 0443 scope A)', () => {
  it('sends a resolved federated address to the account page', async () => {
    mockFetch({
      [TOML]: reply('FEDERATION_SERVER="https://lobstr.co/federation/"\n'),
      [FEDERATION]: reply(JSON.stringify({ account_id: ACCOUNT })),
    });

    renderSearch('karol*lobstr.co');

    expect(await screen.findByText('account page')).toBeInTheDocument();
  });

  // An empty results table would read as "this address does not exist",
  // which is a different claim from "we could not resolve it".
  it('states why a federated address could not be resolved', async () => {
    mockFetch({});

    renderSearch('karol*lobstr.co');

    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent(
        /lobstr\.co did not serve a stellar\.toml/
      );
    });
    expect(screen.queryByText('Transactions')).not.toBeInTheDocument();
  });

  it('leaves an ordinary query alone — no federation request', () => {
    const fetchSpy = vi.fn(() => Promise.reject(new Error('no call expected')));
    vi.stubGlobal('fetch', fetchSpy);

    renderSearch('kale');

    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    for (const [url] of fetchSpy.mock.calls as unknown as [string][]) {
      expect(url).not.toContain('stellar.toml');
    }
  });
});

describe('SearchResultsPage — single hit (task 0527 #2)', () => {
  // 0271 navigated away here. It took the page before the match could be
  // read, and `replace: true` meant Back could not bring it back.
  it('shows the one result instead of navigating to it', async () => {
    renderSearch('one-hit');

    const links = await screen.findAllByRole('link');
    expect(links.map((a) => a.getAttribute('href'))).toContain(
      '/accounts/GA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVSGZ'
    );
    expect(screen.queryByText('account page')).not.toBeInTheDocument();
  });
});
