import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { emptySearchState, renderWithProviders } from '../test-utils.js';
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

describe('GlobalSearchBar — federated addresses (task 0443 scope A)', () => {
  // `/v1/search` knows nothing about SEP-2, so it answers zero hits and the
  // dropdown used to say "No results for karol*lobstr.co" — while Enter goes
  // on to resolve exactly that address.
  it('says what Enter will do instead of claiming no results', () => {
    renderBar('karol*lobstr.co');

    expect(
      screen.getByText(/Resolve this federated address with lobstr\.co/)
    ).toBeInTheDocument();
    expect(screen.queryByText(/No results/)).not.toBeInTheDocument();
  });

  // Suppressing the /v1/search request now belongs to `useSearchResults`
  // itself (it is mocked here), and is covered by its own test. What this
  // component still owns is offering the row as a click target, not only as
  // an Enter shortcut.
  it('offers the federated hint as a clickable row', () => {
    renderBar('karol*lobstr.co');

    const row = screen.getByRole('option');
    expect(row.tagName).toBe('BUTTON');
  });

  it('leaves an ordinary query on the normal results view', () => {
    renderBar('kale');

    expect(
      screen.queryByText(/Resolve this federated address/)
    ).not.toBeInTheDocument();
    expect(seen).toContain('kale');
  });
});
