import { describe, expect, it } from 'vitest';

import { routeForHit } from './routes.js';

const ACCOUNT = 'GBHH24YAUSBA3C5MKMRTDZHU6P4BRYPGDO2RTDNOXERFRBQ7SVXMOQBC';
const CONTRACT = 'CCII7OQKIRNLXZFQ6N4H7CQP7RZO6BMIEPGBF2Y2P6SAVXTBRE7RR275';
const POOL = 'LAXGGSPMJ6CB6WCE3BAJLVXKXHKHKVFS32ERXWAA3BVAXQJ5ZKUCN7IZ';

describe('routeForHit', () => {
  // Strkey-addressed entities carry no `route_token` — their display
  // `identifier` IS the routable id, so routing falls through to it.
  it('routes account by its identifier (no route_token)', () => {
    expect(routeForHit({ entity_type: 'account', identifier: ACCOUNT })).toBe(
      `/accounts/${ACCOUNT}`
    );
  });

  it('routes contract by its identifier (no route_token)', () => {
    expect(routeForHit({ entity_type: 'contract', identifier: CONTRACT })).toBe(
      `/contracts/${CONTRACT}`
    );
  });

  it('routes liquidity pool by its identifier (no route_token)', () => {
    expect(routeForHit({ entity_type: 'pool', identifier: POOL })).toBe(
      `/liquidity-pools/${POOL}`
    );
  });

  // Asset's display `identifier` is the non-routable asset code; the
  // canonical `/assets/:id` token comes from `route_token`.
  it('routes asset by route_token over the display identifier', () => {
    expect(
      routeForHit({
        entity_type: 'asset',
        identifier: 'USDC',
        route_token: `USDC-${ACCOUNT}`,
      })
    ).toBe(`/assets/USDC-${ACCOUNT}`);
  });

  it('falls back to identifier for an asset without route_token', () => {
    expect(routeForHit({ entity_type: 'asset', identifier: 'native' })).toBe(
      '/assets/native'
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
