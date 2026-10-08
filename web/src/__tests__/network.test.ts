import { describe, expect, it } from 'vitest';

import { markTestnetTab, networkFrom, pricesApiUrlFor } from '../network.js';

describe('networkFrom', () => {
  it('reads mainnet when the build sets nothing, as production always has', () => {
    expect(networkFrom(undefined)).toBe('mainnet');
    expect(networkFrom('')).toBe('mainnet');
    expect(networkFrom('mainnet')).toBe('mainnet');
  });

  it('reads testnet', () => {
    expect(networkFrom('testnet')).toBe('testnet');
  });

  it('refuses a misspelt name instead of showing mainnet', () => {
    expect(() => networkFrom('tesnet')).toThrow(/tesnet/);
    expect(() => networkFrom('Testnet')).toThrow();
  });
});

describe('markTestnetTab', () => {
  it('prefixes the title and points every icon at its testnet twin', () => {
    document.head.innerHTML =
      '<link rel="icon" type="image/svg+xml" href="/favicon.svg" />' +
      '<link rel="icon" type="image/png" href="/favicon-32.png" />' +
      '<link rel="apple-touch-icon" href="/apple-touch-icon.png" />' +
      '<link rel="stylesheet" href="/fonts.css" />';
    document.title = 'Soroban Block Explorer';

    markTestnetTab('testnet');

    expect(document.title).toBe('Testnet · Soroban Block Explorer');
    const hrefs = [...document.querySelectorAll('link')].map((l) =>
      l.getAttribute('href')
    );
    expect(hrefs).toEqual([
      '/favicon-testnet.svg',
      '/favicon-testnet-32.png',
      '/apple-touch-icon-testnet.png',
      '/fonts.css',
    ]);
  });

  it('leaves a mainnet tab as it is', () => {
    document.head.innerHTML =
      '<link rel="icon" type="image/svg+xml" href="/favicon.svg" />';
    document.title = 'Soroban Block Explorer';

    markTestnetTab('mainnet');

    expect(document.title).toBe('Soroban Block Explorer');
    expect(document.querySelector('link')?.getAttribute('href')).toBe(
      '/favicon.svg'
    );
  });
});

describe('pricesApiUrlFor', () => {
  it("links the portal on mainnet's own host", () => {
    expect(pricesApiUrlFor('mainnet')).toBe('/prices-api/');
  });

  it("sends testnet to mainnet's portal, since testnet has none", () => {
    expect(pricesApiUrlFor('testnet')).toBe(
      'https://sorobanscan.rumblefish.dev/prices-api/'
    );
  });
});
