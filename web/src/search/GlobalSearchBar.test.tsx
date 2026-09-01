import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../test-utils.js';
import { GlobalSearchBar } from './GlobalSearchBar.js';
import type { SearchResultsState } from './useSearchResults.js';

const seen: string[] = [];

vi.mock('./useSearchResults.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./useSearchResults.js')>();
  return {
    ...actual,
    useSearchResults: vi.fn((params: { q: string }): SearchResultsState => {
      seen.push(params.q);
      return {
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
      };
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

describe('GlobalSearchBar — federated addresses (task 0443 scope A)', () => {
  // `/v1/search` knows nothing about SEP-2, so it answers zero hits and the
  // dropdown used to say "No results for karol*lobstr.co" — while Enter goes
  // on to resolve exactly that address.
  it('says what Enter will do instead of claiming no results', () => {
    renderBar('karol*lobstr.co');

    expect(
      screen.getByText(
        /Press Enter to resolve karol\*lobstr\.co with lobstr\.co/
      )
    ).toBeInTheDocument();
    expect(screen.queryByText(/No results/)).not.toBeInTheDocument();
  });

  it('does not query the search API for a federated address', () => {
    renderBar('karol*lobstr.co');

    expect(seen).not.toContain('karol*lobstr.co');
  });

  it('leaves an ordinary query on the normal results view', () => {
    renderBar('kale');

    expect(
      screen.queryByText(/Press Enter to resolve/)
    ).not.toBeInTheDocument();
    expect(seen).toContain('kale');
  });
});
