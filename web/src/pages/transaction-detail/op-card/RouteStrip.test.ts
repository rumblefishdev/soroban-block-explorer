import type { XdrOperationDto } from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import { parseOperationTree } from './CallTree.js';
import { buildRouteModel } from './RouteStrip.js';

function heavy(details: Record<string, unknown>): XdrOperationDto {
  return { op_type: 'x', application_order: 1, details };
}

describe('buildRouteModel', () => {
  it('chains an all-pool route with per-hop amounts (VELO arbitrage op 2)', () => {
    const model = buildRouteModel(
      heavy({
        sendAsset: 'LIBRE:GA',
        destAsset: 'VELO:GB',
        path: ['BLND:GC', 'LMNR:GD'],
        claimedAtoms: [
          { assetSold: 'BLND:GC', amountSold: 1_181_422 },
          { assetSold: 'LMNR:GD', amountSold: 2_976_744_143 },
          { assetSold: 'VELO:GB', amountSold: 13_802_682 },
        ],
      })
    );
    expect(model?.chips).toEqual(['LIBRE', 'BLND', 'LMNR', 'VELO']);
    expect(model?.edges.map((e) => e.label)).toEqual([
      '0.1181422 BLND',
      '297.6744143 LMNR',
      '1.3802682 VELO',
    ]);
    expect(model?.partial).toBe(false);
  });

  it('keeps the DECLARED chain on a mixed route and leaves order-book hops unlabelled', () => {
    const model = buildRouteModel(
      heavy({
        sendAsset: 'VELO:GB',
        destAsset: 'LIBRE:GA',
        path: ['native', 'KALE:GK'],
        claimedAtoms: [
          { assetSold: 'KALE:GK', amountSold: 861_037_745 },
          { assetSold: 'LIBRE:GA', amountSold: 48_215_990 },
        ],
      })
    );
    // The XLM hop crossed the order book — it stays IN the chain, only its
    // amount is unknown (the old model dropped the asset entirely).
    expect(model?.chips).toEqual(['VELO', 'XLM', 'KALE', 'LIBRE']);
    expect(model?.edges.map((e) => e.label)).toEqual([
      null,
      '86.1037745 KALE',
      '4.821599 LIBRE',
    ]);
    expect(model?.partial).toBe(true);
    expect(model?.hasFills).toBe(true);
  });

  it('renders the declared route without blaming the order book when no fills exist', () => {
    const model = buildRouteModel(
      heavy({ sendAsset: 'native', destAsset: 'USDC:GA', path: [] })
    );
    expect(model?.chips).toEqual(['XLM', 'USDC']);
    expect(model?.partial).toBe(true);
    expect(model?.hasFills).toBe(false);
  });

  it('returns null for non-path-payment shapes', () => {
    expect(buildRouteModel(heavy({ amount: 1, asset: 'native' }))).toBeNull();
    expect(buildRouteModel(null)).toBeNull();
  });
});

describe('parseOperationTree', () => {
  it('parses nodes defensively and keeps the per-node field without rendering it', () => {
    const nodes = parseOperationTree([
      {
        contractId: 'CDL7',
        functionName: 'plant',
        args: [1, 2],
        returnValue: null,
        successful: true,
        children: [
          {
            contractId: 'CDL7',
            functionName: 'burn',
            args: [1],
            successful: false,
            children: [],
          },
          'garbage',
        ],
      },
      42,
    ]);
    expect(nodes).toHaveLength(1);
    expect(nodes[0].functionName).toBe('plant');
    expect(nodes[0].argCount).toBe(2);
    expect(nodes[0].children).toHaveLength(1);
    expect(nodes[0].children[0].successful).toBe(false);
  });

  it('returns [] for non-arrays', () => {
    expect(parseOperationTree(undefined)).toEqual([]);
    expect(parseOperationTree({})).toEqual([]);
  });
});
