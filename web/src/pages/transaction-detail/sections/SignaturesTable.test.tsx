import { render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it } from 'vitest';

import { SignaturesTable } from './SignaturesTable.js';

function renderTable(props: Parameters<typeof SignaturesTable>[0]) {
  return render(
    <ExplorerThemeProvider>
      <SignaturesTable {...props} />
    </ExplorerThemeProvider>
  );
}

describe('SignaturesTable', () => {
  // An empty list reaches this component from two indistinguishable causes —
  // the archive fetch failed, or it answered while this transaction's envelope
  // was missing, which still yields a heavy block with `signatures: []`. It
  // must therefore never claim the transaction has none (0377 F1).
  it('reports an empty list as unreadable, not as "none"', () => {
    renderTable({ signatures: [] });

    expect(screen.getByText('Signatures unavailable')).toBeInTheDocument();
    expect(screen.queryByText('No signatures recorded.')).toBeNull();
    expect(screen.queryByText('0 signatures')).toBeNull();
  });

  it('renders the signer rows when signatures are present', () => {
    renderTable({
      signatures: [{ hint: 'aabbccdd', signature: 'ab'.repeat(32), weight: 1 }],
    });

    expect(screen.getByText('1 signature')).toBeInTheDocument();
  });
});
