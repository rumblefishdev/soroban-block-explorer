import { describe, expect, it } from 'vitest';

import { routes } from './routes.js';

describe('routes (canonical URL-shape table)', () => {
  it('exposes list routes as plain paths', () => {
    expect(routes.home).toBe('/');
    expect(routes.transactions).toBe('/transactions');
    expect(routes.pools).toBe('/liquidity-pools');
  });

  it('builds single-entity detail URLs', () => {
    expect(routes.account('GABC')).toBe('/accounts/GABC');
    expect(routes.contract('CABC')).toBe('/contracts/CABC');
    expect(routes.ledger(12345)).toBe('/ledgers/12345');
    expect(routes.asset('native')).toBe('/assets/native');
  });

  it('builds the NFT composite (contract_id, token_id) URL', () => {
    expect(routes.nft('CABC', 'token-1')).toBe('/nfts/CABC/token-1');
  });

  it('percent-encodes id args uniformly (drift fix, task 0299)', () => {
    // `/`, `?`, `#`, space must never break the path — encoded everywhere,
    // not just on some entities like the old duplicate tables did.
    expect(routes.pool('L/A?B#C')).toBe('/liquidity-pools/L%2FA%3FB%23C');
    expect(routes.nft('C/1', 't 2')).toBe('/nfts/C%2F1/t%202');
    expect(routes.search('a b&c')).toBe('/search?q=a%20b%26c');
  });
});
