import { screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  emptySearchState,
  fetchReply,
  renderWithProviders,
  stubFetch,
} from '../test-utils.js';
import { GlobalSearchBar } from './GlobalSearchBar.js';
import type { SearchResultsState } from './useSearchResults.js';

const seen: string[] = [];

vi.mock('./useSearchResults.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./useSearchResults.js')>();
  return {
    ...actual,
    useSearchResults: vi.fn((params: { q: string }): SearchResultsState => {
      seen.push(params.q);
      return emptySearchState(params.q);
    }),
  };
});

function renderBar(q: string) {
  seen.length = 0;
  return renderWithProviders(
    <GlobalSearchBar
      q={q}
      onDismiss={() => undefined}
      registerEnterHandler={() => undefined}
    />
  );
}

const ACCOUNT = 'GC526FUILJ6NLFXKCOOGTMDXNRW7MYSEK2UNRJV5FYWOGYDE4LOKXFEM';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('GlobalSearchBar — federated addresses (task 0443 scope A)', () => {
  // `/v1/search` knows nothing about SEP-2, so it answers zero hits and the
  // dropdown would otherwise say "No results for karol*lobstr.co" — the one
  // claim that is false about an address that resolves.
  it('resolves the address in the dropdown instead of claiming no results', async () => {
    stubFetch({
      'https://lobstr.co/.well-known/stellar.toml': fetchReply(
        'FEDERATION_SERVER="https://lobstr.co/federation/"\n'
      ),
      'https://lobstr.co/federation/': fetchReply(
        JSON.stringify({ account_id: ACCOUNT })
      ),
    });

    renderBar('karol*lobstr.co');

    // While the two hops are in flight the row says so, and says which domain
    // is being asked — never "no results".
    expect(
      screen.getByText(/Resolving karol\*lobstr\.co with lobstr\.co/)
    ).toBeInTheDocument();
    expect(screen.queryByText(/No results/)).not.toBeInTheDocument();

    // Then the account itself, as an ordinary-looking result row.
    expect(await screen.findByText(ACCOUNT)).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByRole('option')).toBeEnabled();
    });
  });

  // A dead domain must say which hop failed, in the dropdown, rather than
  // leaving the row spinning or falling back to an empty list.
  it('names the failed hop when the domain serves no stellar.toml', async () => {
    stubFetch({});

    renderBar('karol*lobstr.co');

    expect(
      await screen.findByText(/did not serve a stellar\.toml/)
    ).toBeInTheDocument();
    expect(screen.getByRole('option')).toBeDisabled();
  });

  // Suppressing the /v1/search request now belongs to `useSearchResults`
  // itself (it is mocked here). What this component still owns is offering
  // the row as a click target, not only as an Enter shortcut.
  it('offers the federated row as a click target', () => {
    stubFetch({});
    renderBar('karol*lobstr.co');

    const row = screen.getByRole('option');
    expect(row.tagName).toBe('BUTTON');
  });

  it('leaves an ordinary query on the normal results view', () => {
    const fetchMock = stubFetch({});
    renderBar('kale');

    expect(screen.queryByText(/Resolving/)).not.toBeInTheDocument();
    // An ordinary query must not reach a third-party host at all.
    expect(fetchMock).not.toHaveBeenCalled();
    expect(seen).toContain('kale');
  });
});
