import { fireEvent, render, screen } from '@testing-library/react';
import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { describe, expect, it, vi } from 'vitest';

import { HighlightedJson } from '../HighlightedJson.js';

const CONTRACT = 'CDDTJ7OZU2WZAEZNTUZWIRAE4EMP5CF63M3INFQWTLX4ENMYUFK6RCTX';
const ACCOUNT = 'GC4QMEH5CY5HAEZVC2XNTRV2XBPQWUX2WCV3ANU32HBFNCYIKWHGK7XQ';

/** What the copy button puts on the clipboard: the full value, never the
 *  shortened text on screen. */
function copiedByButton(): string {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', {
    value: { writeText },
    configurable: true,
  });
  fireEvent.click(screen.getByRole('button', { name: /copy/i }));
  return writeText.mock.calls[0][0];
}

function renderJson(value: unknown, narrow = false) {
  return render(
    <ExplorerThemeProvider>
      <HighlightedJson value={value} narrow={narrow} />
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

  it('shortens an address in a narrow column, keeping its link and copy whole', () => {
    renderJson([{ type: 'address', value: ACCOUNT }], true);
    const link = screen.getByRole('link', { name: ACCOUNT });
    expect(link.textContent).toBe('GC4Q…K7XQ');
    expect(link.getAttribute('href')).toBe(`/accounts/${ACCOUNT}`);
    expect(copiedByButton()).toBe(ACCOUNT);
  });

  it('shortens only the issuer of an asset in a narrow column', () => {
    const asset = `yUSDC:${ACCOUNT}`;
    renderJson({ asset }, true);
    const link = screen.getByRole('link', { name: asset });
    expect(link.textContent).toBe('yUSDC:GC4Q…K7XQ');
    expect(link.getAttribute('href')).toBe(`/assets/yUSDC-${ACCOUNT}`);
    expect(copiedByButton()).toBe(asset);
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
