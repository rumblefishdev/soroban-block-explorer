import type { XdrEventDto } from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import {
  buildExecutionTrace,
  contractStrkeyFromBase64,
  traceCallCount,
} from './ExecutionTrace.js';

// Real pair from mainnet tx 54aab000…b21f2d: the fn_call bytes topic and the
// C-strkey of the contract that raised events inside that call.
const CDDT_BYTES = 'xzT92aatkBMtnTNkRAThGP6Ivts2hpYWmu/CNZihVeg=';
const CDDT = 'CDDTJ7OZU2WZAEZNTUZWIRAE4EMP5CF63M3INFQWTLX4ENMYUFK6RCTX';

let nextIndex = 0;
function ev(
  topics: { type: string; value?: unknown }[],
  data: unknown = null,
  contract_id: string | null = null
): XdrEventDto {
  return {
    event_type: 'diagnostic',
    contract_id,
    topics,
    data,
    event_index: nextIndex++,
    op_index: null,
  } as XdrEventDto;
}

const fnCall = (name: string, bytes = CDDT_BYTES, data: unknown = null) =>
  ev(
    [
      { type: 'sym', value: 'fn_call' },
      { type: 'bytes', value: bytes },
      { type: 'sym', value: name },
    ],
    data
  );
const fnReturn = (name: string, data: unknown = null) =>
  ev(
    [
      { type: 'sym', value: 'fn_return' },
      { type: 'sym', value: name },
    ],
    data
  );

describe('contractStrkeyFromBase64', () => {
  it('encodes the fixture bytes to the known C-strkey', () => {
    expect(contractStrkeyFromBase64(CDDT_BYTES)).toBe(CDDT);
  });

  it('rejects malformed input instead of throwing', () => {
    expect(contractStrkeyFromBase64('not-base64!!')).toBeNull();
    expect(contractStrkeyFromBase64('c2hvcnQ=')).toBeNull();
  });
});

describe('buildExecutionTrace', () => {
  it('rebuilds nesting, attaches events to the active call, sets returns', () => {
    // swap(A) { burn_and_transfer(B) { [burn event] } ; balance(C) }
    const burn = ev(
      [{ type: 'sym', value: 'burn' }],
      { type: 'i128', value: '12958' },
      CDDT
    );
    const nodes = buildExecutionTrace([
      fnCall('swap', CDDT_BYTES, { type: 'vec', value: [1, 2] }),
      fnCall('burn_and_transfer'),
      burn,
      fnReturn('burn_and_transfer', { type: 'void' }),
      fnCall('balance'),
      fnReturn('balance', { type: 'i128', value: '81813607' }),
      fnReturn('swap', { type: 'u128', value: '81404538' }),
    ]);

    expect(nodes).toHaveLength(1);
    const swap = nodes[0];
    expect(swap.fnName).toBe('swap');
    expect(swap.contractId).toBe(CDDT);
    expect(swap.returnValue).toEqual({ type: 'u128', value: '81404538' });
    expect(swap.unfinished).toBe(false);
    expect(swap.children.map((c) => c.fnName)).toEqual([
      'burn_and_transfer',
      'balance',
    ]);
    expect(swap.children[0].events).toEqual([burn]);
    expect(swap.children[1].events).toEqual([]);
    expect(traceCallCount(nodes)).toBe(3);
  });

  it('skips core_metrics and ignores events outside any call', () => {
    const nodes = buildExecutionTrace([
      ev([{ type: 'sym', value: 'fee' }]),
      fnCall('swap'),
      ev([{ type: 'sym', value: 'core_metrics' }], { type: 'u64', value: '1' }),
      fnReturn('swap'),
    ]);
    expect(traceCallCount(nodes)).toBe(1);
    expect(nodes[0].events).toEqual([]);
  });

  it('marks calls still on the stack as unfinished (failed tx trace)', () => {
    const nodes = buildExecutionTrace([
      fnCall('swap'),
      fnCall('transfer'),
      // trap: no fn_return for either
    ]);
    expect(nodes[0].unfinished).toBe(true);
    expect(nodes[0].children[0].unfinished).toBe(true);
  });

  it('tolerates a stray fn_return and non-diagnostic noise', () => {
    const nodes = buildExecutionTrace([
      fnReturn('ghost'),
      fnCall('swap'),
      fnReturn('swap'),
    ]);
    expect(traceCallCount(nodes)).toBe(1);
    expect(nodes[0].unfinished).toBe(false);
  });
});
