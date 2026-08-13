import type { ContractDetailResponse } from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import { routes } from '../../router/routes.js';

import { contractFace } from './contractFace.js';

const CONTRACT = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
const ISSUER = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN';

function makeContract(
  overrides: Partial<ContractDetailResponse> = {}
): ContractDetailResponse {
  return {
    contract_id: CONTRACT,
    contract_type: 0,
    contract_type_name: 'token',
    is_sac: false,
    sac_asset: null,
    deployer: null,
    deployed_at_ledger: 50_463_389,
    wasm_hash: null,
    stats: {
      recent_events: 0,
      recent_invocations: 0,
      recent_unique_callers: 0,
      stats_window: '7 days',
    },
    ...overrides,
  };
}

describe('contractFace (task 0472)', () => {
  it('names and links the asset a SAC mirrors', () => {
    const face = contractFace(
      makeContract({
        is_sac: true,
        sac_asset: { asset_code: 'USDC', issuer: ISSUER },
      })
    );
    expect(face.label).toBe('Stellar Asset Contract · USDC');
    expect(face.href).toBe(routes.asset(`USDC-${ISSUER}`));
    expect(face.title).toContain(ISSUER);
  });

  it('routes the native XLM SAC to the canonical asset token', () => {
    const face = contractFace(
      makeContract({
        is_sac: true,
        sac_asset: { asset_code: null, issuer: null },
      })
    );
    expect(face.label).toBe('Stellar Asset Contract · XLM');
    expect(face.href).toBe(routes.asset('native'));
  });

  it('links a Fungible contract to its own asset page', () => {
    const face = contractFace(
      makeContract({ contract_type: 3, contract_type_name: 'fungible' })
    );
    expect(face.label).toBe('Fungible');
    expect(face.href).toBe(routes.asset(CONTRACT));
  });

  it('links an NFT contract to its filtered collection view', () => {
    const face = contractFace(
      makeContract({ contract_type: 2, contract_type_name: 'nft' })
    );
    expect(face.label).toBe('NFT');
    expect(face.href).toBe(routes.nftsByContract(CONTRACT));
    // The frontend filter key is `contract`, NOT the filter[contract_id] API
    // param — NftsListPage maps one to the other itself.
    expect(face.href).toContain('?contract=');
  });

  it('leaves Other unlinked — there is nothing to point at', () => {
    const face = contractFace(
      makeContract({ contract_type: 1, contract_type_name: 'other' })
    );
    expect(face.label).toBe('Other');
    expect(face.href).toBeUndefined();
  });

  it('degrades a SAC with an unresolvable facet to a bare, unlinked chip', () => {
    const face = contractFace(makeContract({ is_sac: true, sac_asset: null }));
    expect(face.label).toBe('Stellar Asset Contract');
    expect(face.href).toBeUndefined();
  });
});
