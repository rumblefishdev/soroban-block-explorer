import { screen, waitFor } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  emptySearchState,
  fetchReply,
  renderWithProviders,
  stubFetch,
} from '../test-utils.js';
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
          ? emptySearchState(params.q, {
              ...ONE_HIT_STATE,
              // `SearchResultsView` renders rows only when `data` is present.
              data: {
                groups: { accounts: ONE_HIT_STATE.hitsForActiveTab },
              } as SearchResultsState['data'],
            })
          : emptySearchState(params.q)
    ),
  };
});

const ACCOUNT = 'GC526FUILJ6NLFXKCOOGTMDXNRW7MYSEK2UNRJV5FYWOGYDE4LOKXFEM';
const TOML = 'https://lobstr.co/.well-known/stellar.toml';
const FEDERATION = 'https://lobstr.co/federation/';

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
    stubFetch({
      [TOML]: fetchReply('FEDERATION_SERVER="https://lobstr.co/federation/"\n'),
      [FEDERATION]: fetchReply(JSON.stringify({ account_id: ACCOUNT })),
    });

    renderSearch('karol*lobstr.co');

    expect(await screen.findByText('account page')).toBeInTheDocument();
  });

  // An empty results table would read as "this address does not exist",
  // which is a different claim from "we could not resolve it".
  it('states why a federated address could not be resolved', async () => {
    stubFetch({});

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

describe('SearchResultsPage — query input (task 0527 #1)', () => {
  it('leaves the caret where it is when editing mid-query', async () => {
    const user = userEvent.setup();
    renderSearch('kale');

    const input = screen.getByLabelText(
      'Search by TX hash, accounts, contract, token'
    ) as HTMLInputElement;
    // Put the caret between "ka" and "le" and type one character there.
    await user.type(input, 'X', {
      initialSelectionStart: 2,
      initialSelectionEnd: 2,
    });

    expect(input.value).toBe('kaXle');
    expect(input.selectionStart).toBe(3);
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

describe('SearchResultsPage — federation lookups are debounced', () => {
  // Typing `bob*lobstr.com` passes through `bob*lobstr.co` — a real domain,
  // a valid federated shape, and not the one the user meant. Undebounced,
  // that domain receives a request and with it the viewer's IP.
  it('does not contact a domain the user was still typing past', async () => {
    const user = userEvent.setup();
    const fetchMock = vi.fn(() => Promise.reject(new Error('ENOTFOUND')));
    vi.stubGlobal('fetch', fetchMock);

    // Seeded with a plain query so the collapsed input is already expanded.
    renderSearch('bob');

    const input = screen.getByLabelText(
      'Search by TX hash, accounts, contract, token'
    );
    await user.type(input, '*lobstr.com');

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalled();
    });
    const urls = (fetchMock.mock.calls as unknown as [string][]).map(
      ([url]) => url
    );
    expect(urls.some((u) => u.startsWith('https://lobstr.co/'))).toBe(false);
    expect(urls.some((u) => u.startsWith('https://lobstr.com/'))).toBe(true);
  });
});
