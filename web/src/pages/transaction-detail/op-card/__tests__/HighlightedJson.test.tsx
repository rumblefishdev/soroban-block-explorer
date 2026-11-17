import { render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it } from 'vitest';

import { HighlightedJson } from './HighlightedJson.js';

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

  it('links canonical CODE:ISSUER assets to the asset page (dash route)', () => {
    const asset = `USDC:${ACCOUNT}`;
    renderJson({ asset });
    expect(screen.getByRole('link', { name: asset }).getAttribute('href')).toBe(
      `/assets/USDC-${ACCOUNT}`
    );
  });

  it('leaves ordinary strings and near-miss values untouched', () => {
    renderJson({
      note: 'hello',
      native: 'native',
      // 55 chars after prefix required — this one is short.
      short: 'CDDTJ7OZU2WZAEZNTUZWIRAE4EMP5CF63M3INFQWTLX4ENMYUFK6',
      badAsset: 'USDC:GSHORT',
      bytes: 'p0TleCgt/LYD1K7KZQcNKJSgWIqsvKfSV6Zmgs27koo=',
    });
    expect(screen.queryByRole('link')).toBeNull();
    expect(screen.queryByRole('button')).toBeNull();
  });
});
