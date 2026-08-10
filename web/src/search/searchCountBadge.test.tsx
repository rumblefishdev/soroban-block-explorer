import { render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it, vi } from 'vitest';

import { SearchResultsTabs } from './SearchResultsTabs.js';
import { SEARCH_GROUP_LIMIT } from './useSearchResults.js';

function renderTabs(transactionCount: number) {
  return render(
    <ExplorerThemeProvider>
      <SearchResultsTabs
        activeTab="account"
        onChange={vi.fn()}
        counts={{
          transaction: transactionCount,
          account: 1,
          contract: 0,
          asset: 0,
          nft: 0,
          pool: 0,
        }}
      />
    </ExplorerThemeProvider>
  );
}

/**
 * A bucket that reaches the server cap holds "at least N", not "exactly N" —
 * the badge has to say so or a truncated bucket reads as a total (0377 F7).
 */
describe('search count badge', () => {
  it('shows a bare count below the cap', () => {
    renderTabs(SEARCH_GROUP_LIMIT - 1);

    expect(
      screen.getByText(String(SEARCH_GROUP_LIMIT - 1))
    ).toBeInTheDocument();
    expect(screen.queryByText(`${SEARCH_GROUP_LIMIT - 1}+`)).toBeNull();
  });

  it('marks a saturated bucket with a trailing plus', () => {
    renderTabs(SEARCH_GROUP_LIMIT);

    expect(screen.getByText(`${SEARCH_GROUP_LIMIT}+`)).toBeInTheDocument();
  });
});
