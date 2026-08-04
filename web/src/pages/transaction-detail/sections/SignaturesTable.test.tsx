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
  it('says the signatures could not be loaded when heavy is unavailable', () => {
    renderTable({ signatures: [], unavailable: true });

    expect(screen.getByText('Signatures unavailable')).toBeInTheDocument();
    // The whole point of 0377 F1: an applied tx always has >=1 signature, so
    // claiming zero here would state something impossible.
    expect(screen.queryByText('No signatures recorded.')).toBeNull();
    expect(screen.queryByText('0 signatures')).toBeNull();
  });

  it('still reports a genuine empty list when heavy did load', () => {
    renderTable({ signatures: [], unavailable: false });

    expect(screen.getByText('No signatures recorded.')).toBeInTheDocument();
    expect(screen.queryByText('Signatures unavailable')).toBeNull();
  });

  it('renders the signer rows when signatures are present', () => {
    renderTable({
      signatures: [{ hint: 'aabbccdd', signature: 'ab'.repeat(32), weight: 1 }],
      unavailable: false,
    });

    expect(screen.getByText('1 signature')).toBeInTheDocument();
  });
});
