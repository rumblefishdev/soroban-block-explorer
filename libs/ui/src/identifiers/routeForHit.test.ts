import { describe, expect, it } from 'vitest';

import { routeForHit } from './routes.js';

const ACCOUNT = 'GBHH24YAUSBA3C5MKMRTDZHU6P4BRYPGDO2RTDNOXERFRBQ7SVXMOQBC';
const CONTRACT = 'CCII7OQKIRNLXZFQ6N4H7CQP7RZO6BMIEPGBF2Y2P6SAVXTBRE7RR275';
const POOL = 'LAXGGSPMJ6CB6WCE3BAJLVXKXHKHKVFS32ERXWAA3BVAXQJ5ZKUCN7IZ';

describe('routeForHit', () => {
  // F-RR-35: strkey-addressed entities must route by `identifier`, NOT the
  // numeric `surrogate_id` (their detail endpoints reject the numeric form).
  it('routes account by StrKey identifier even when surrogate_id present', () => {
    expect(
      routeForHit({
        entity_type: 'account',
        identifier: ACCOUNT,
        surrogate_id: 15805579,
      })
    ).toBe(`/accounts/${ACCOUNT}`);
  });

  it('routes contract by StrKey identifier even when surrogate_id present', () => {
    expect(
      routeForHit({
        entity_type: 'contract',
        identifier: CONTRACT,
        surrogate_id: 2154201,
      })
    ).toBe(`/contracts/${CONTRACT}`);
  });

  it('routes liquidity pool by StrKey identifier even when surrogate_id present', () => {
    expect(
      routeForHit({ entity_type: 'pool', identifier: POOL, surrogate_id: 99 })
    ).toBe(`/liquidity-pools/${POOL}`);
  });

  // Asset is the sole entity whose route (`/v1/assets/:id`) accepts the
  // numeric surrogate — keep that path.
  it('routes asset by surrogate_id when present', () => {
    expect(
      routeForHit({ entity_type: 'asset', identifier: 'USDC', surrogate_id: 42 })
    ).toBe('/assets/42');
  });

  it('falls back to identifier for asset without surrogate_id', () => {
    expect(routeForHit({ entity_type: 'asset', identifier: '42' })).toBe(
      '/assets/42'
    );
  });

  it('builds composite NFT url from contract_id + token_id', () => {
    expect(
      routeForHit({
        entity_type: 'nft',
        identifier: 'ignored',
        contract_id: CONTRACT,
        token_id: '7',
      })
    ).toBe(`/nfts/${CONTRACT}/7`);
  });
});
