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

describe('buildExecutionTrace — pre-P23 emitter-match fallback', () => {
  const CDDT2_BYTES = 'TSCrDMO85hjwvdrgxNWgy0HUWrrFuV3N7HUm3QuP7iU=';

  const contractEvent = (emitter: string | null) =>
    ev(
      [{ type: 'sym', value: 'transfer' }],
      { type: 'i128', value: '5' },
      emitter
    );

  it('attaches a tx-level event when exactly one call matches its emitter', () => {
    const nodes = buildExecutionTrace(
      [fnCall('set_price'), fnReturn('set_price', { type: 'void' })],
      [contractEvent(CDDT)]
    );
    expect(nodes[0].children).toHaveLength(1);
    expect(nodes[0].children[0].kind).toBe('event');
  });

  it('leaves ambiguous and unmatched events in the Events section', () => {
    const nodes = buildExecutionTrace(
      [
        fnCall('a'),
        fnReturn('a'),
        fnCall('b'),
        fnReturn('b'),
        fnCall('other', CDDT2_BYTES),
        fnReturn('other'),
      ],
      [
        contractEvent(CDDT), // two calls on CDDT → ambiguous
        // no call on this contract (like a tx-level fee event) → unmatched
        contractEvent(
          'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75'
        ),
      ]
    );
    const eventChildren = (n: (typeof nodes)[number]) =>
      n.children.filter((c) => c.kind === 'event');
    expect(nodes.flatMap(eventChildren)).toHaveLength(0);
  });

  it('never double-attaches when the stream already carries copies', () => {
    const copy = contractEvent(CDDT);
    const nodes = buildExecutionTrace(
      [fnCall('swap'), copy, fnReturn('swap')],
      [contractEvent(CDDT)]
    );
    expect(nodes[0].children.filter((c) => c.kind === 'event')).toHaveLength(1);
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
    expect(eventArgsText(error)).toBe(
      '("failing with contract error", +1 field)'
    );
  });

  it('keeps the message of a diagnostic that cannot inline (live trap fixture)', () => {
    // f0ec7986…: message + emitter + storage key + type — four fields, so the
    // generic path would have shown "(4 fields)" and hidden the reason.
    const error = ev(
      [
        { type: 'sym', value: 'error' },
        { type: 'error', value: 'Storage' },
      ],
      {
        type: 'vec',
        value: [
          {
            type: 'string',
            value:
              'trying to access contract data key outside of the footprint',
          },
          { type: 'address', value: CDDT },
          { type: 'vec', value: [{ type: 'sym', value: 'Block' }] },
        ],
      }
    );
    expect(eventArgsText(error)).toBe(
      '("trying to access contract data key outside of the footprint", +3 fields)'
    );
  });

  it('clips an over-long diagnostic message instead of dropping it', () => {
    const error = ev([{ type: 'sym', value: 'error' }], {
      type: 'string',
      value: 'x'.repeat(200),
    });
    const text = eventArgsText(error);
    expect(text.startsWith('("xxx')).toBe(true);
    expect(text.endsWith('…")')).toBe(true);
    expect(text.length).toBeLessThan(80);
  });

  it('falls back to a field count when the payload cannot inline', () => {
    const long = ev(
      [
        { type: 'sym', value: 'trade' },
        { type: 'string', value: 'x'.repeat(60) },
      ],
      { type: 'i128', value: '5' }
    );
    expect(eventArgsText(long)).toBe('(2 fields)');
  });
});
