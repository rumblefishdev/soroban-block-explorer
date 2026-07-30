import type { XdrEventDto } from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import {
  argsSummary,
  buildExecutionTrace,
  contractStrkeyFromBase64,
  eventArgsText,
  partsToText,
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
  it('rebuilds nesting with events interleaved in stream order', () => {
    // swap(A) { burn ; burn_and_transfer(B) ; transfer ; balance(C) } — the
    // burn fires BEFORE the sub-call, the transfer between the two
    // sub-calls; that chronology must survive as child order.
    const burn = ev(
      [{ type: 'sym', value: 'burn' }],
      { type: 'i128', value: '12958' },
      CDDT
    );
    const transfer = ev(
      [{ type: 'sym', value: 'transfer' }],
      { type: 'i128', value: '13171' },
      CDDT
    );
    const nodes = buildExecutionTrace([
      fnCall('swap', CDDT_BYTES, { type: 'vec', value: [1, 2] }),
      burn,
      fnCall('burn_and_transfer'),
      fnReturn('burn_and_transfer', { type: 'void' }),
      transfer,
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
    expect(
      swap.children.map((child) =>
        child.kind === 'call'
          ? child.node.fnName
          : `ev:${child.event.event_index}`
      )
    ).toEqual([
      `ev:${burn.event_index}`,
      'burn_and_transfer',
      `ev:${transfer.event_index}`,
      'balance',
    ]);
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
    expect(nodes[0].children).toEqual([]);
  });

  it('marks calls still on the stack as unfinished (failed tx trace)', () => {
    const nodes = buildExecutionTrace([
      fnCall('swap'),
      fnCall('transfer'),
      // trap: no fn_return for either
    ]);
    expect(nodes[0].unfinished).toBe(true);
    const child = nodes[0].children[0];
    expect(child.kind).toBe('call');
    if (child.kind === 'call') expect(child.node.unfinished).toBe(true);
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

describe('argsSummary', () => {
  it('inlines a single scalar argument (the host does not wrap it in a vec)', () => {
    const summary = argsSummary({ type: 'address', value: CDDT });
    expect(summary.kind).toBe('inline');
    if (summary.kind === 'inline') {
      expect(partsToText(summary.parts)).toBe('CDDT…RCTX');
    }
  });

  it('falls back to a count for a single non-inlinable argument', () => {
    expect(argsSummary({ type: 'map', value: [] })).toEqual({
      kind: 'count',
      count: 1,
    });
  });
});

describe('eventArgsText', () => {
  it('renders payload topics and data scalars inline', () => {
    const transfer = ev(
      [
        { type: 'sym', value: 'transfer' },
        {
          type: 'address',
          value: 'GC4QMEH5CY5HAEZVC2XNTRV2XBPQWUX2WCV3ANU32HBFNCYIKWHGK7XQ',
        },
        { type: 'address', value: CDDT },
      ],
      { type: 'i128', value: '13171' }
    );
    expect(eventArgsText(transfer)).toBe('(GC4Q…K7XQ, CDDT…RCTX, 13171)');
  });

  it('renders error diagnostics with quoted message and code', () => {
    const error = ev([{ type: 'sym', value: 'error' }], {
      type: 'vec',
      value: [
        { type: 'string', value: 'failing with contract error' },
        { type: 'u32', value: 7 },
      ],
    });
    expect(eventArgsText(error)).toBe('("failing with contract error", 7)');
  });

  it('elides values that do not fit instead of dropping the row', () => {
    const long = ev(
      [
        { type: 'sym', value: 'trade' },
        { type: 'string', value: 'x'.repeat(60) },
      ],
      { type: 'i128', value: '5' }
    );
    expect(eventArgsText(long)).toBe('(5, …)');
  });
});
