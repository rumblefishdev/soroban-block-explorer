import { render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it } from 'vitest';

import { HighlightedJson } from './HighlightedJson.js';
import { TxKnownIdsContext } from './strkeyDecode.js';

// Real pair: these 32 bytes (base64) ARE this contract id in strkey form.
const CDDT_BYTES = 'xzT92aatkBMtnTNkRAThGP6Ivts2hpYWmu/CNZihVeg=';
const CDDT_KEY = 'CDDTJ7OZU2WZAEZNTUZWIRAE4EMP5CF63M3INFQWTLX4ENMYUFK6RCTX';

const CONTRACT = 'CDDTJ7OZU2WZAEZNTUZWIRAE4EMP5CF63M3INFQWTLX4ENMYUFK6RCTX';
const ACCOUNT = 'GC4QMEH5CY5HAEZVC2XNTRV2XBPQWUX2WCV3ANU32HBFNCYIKWHGK7XQ';

function renderJson(value: unknown) {
  return render(
    <ExplorerThemeProvider>
      <HighlightedJson value={value} />
    </ExplorerThemeProvider>
  );
}

describe('HighlightedJson strkey awareness (0460 #14)', () => {
  it('renders contract strkeys as links with a copy button', () => {
    renderJson({ type: 'address', value: CONTRACT });
    const link = screen.getByRole('link', { name: CONTRACT });
    expect(link.getAttribute('href')).toBe(`/contracts/${CONTRACT}`);
    expect(screen.getByRole('button', { name: /copy/i })).toBeTruthy();
  });

  it('routes account strkeys to the account page', () => {
    renderJson([ACCOUNT]);
    expect(
      screen.getByRole('link', { name: ACCOUNT }).getAttribute('href')
    ).toBe(`/accounts/${ACCOUNT}`);
  });

  it('decodes 32-byte bytes values ONLY when corroborated in the tx', () => {
    // Corroborated: the decoded strkey occurs elsewhere in the transaction.
    render(
      <ExplorerThemeProvider>
        <TxKnownIdsContext.Provider value={new Set([CDDT_KEY])}>
          <HighlightedJson value={{ type: 'bytes', value: CDDT_BYTES }} />
        </TxKnownIdsContext.Provider>
      </ExplorerThemeProvider>
    );
    expect(
      screen.getByRole('link', { name: CDDT_KEY }).getAttribute('href')
    ).toBe(`/contracts/${CDDT_KEY}`);
  });

  it('never decodes bytes without corroboration (any 32 bytes "decode")', () => {
    renderJson({ type: 'bytes', value: CDDT_BYTES });
    expect(screen.queryByRole('link')).toBeNull();
  });

  it('leaves ordinary strings and near-miss values untouched', () => {
    renderJson({
      note: 'hello',
      // 55 chars after prefix required — this one is short.
      short: 'CDDTJ7OZU2WZAEZNTUZWIRAE4EMP5CF63M3INFQWTLX4ENMYUFK6',
      bytes: 'p0TleCgt/LYD1K7KZQcNKJSgWIqsvKfSV6Zmgs27koo=',
    });
    expect(screen.queryByRole('link')).toBeNull();
    expect(screen.queryByRole('button')).toBeNull();
  });
});
