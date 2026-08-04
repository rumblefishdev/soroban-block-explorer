import type { XdrEventDto } from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import { allResourceFacts, readResourceCounters } from './resources.js';

function counter(name: string, value: number): XdrEventDto {
  return {
    event_type: 'diagnostic',
    contract_id: null,
    topics: [
      { type: 'sym', value: 'core_metrics' },
      { type: 'sym', value: name },
    ],
    data: { type: 'u64', value },
    event_index: 0,
    op_index: null,
    stage: null,
  } as unknown as XdrEventDto;
}

function fnCall(): XdrEventDto {
  return {
    event_type: 'diagnostic',
    contract_id: null,
    topics: [{ type: 'sym', value: 'fn_call' }],
    data: { type: 'void', value: null },
    event_index: 1,
    op_index: null,
    stage: null,
  } as unknown as XdrEventDto;
}

/** The real counter set from mainnet `0a120260…c38e`. */
const REAL: Array<[string, number]> = [
  ['read_entry', 8],
  ['write_entry', 3],
  ['ledger_read_byte', 116],
  ['ledger_write_byte', 412],
  ['read_key_byte', 84],
  ['write_key_byte', 0],
  ['read_data_byte', 116],
  ['write_data_byte', 412],
  ['read_code_byte', 0],
  ['write_code_byte', 0],
  ['emit_event', 1],
  ['emit_event_byte', 248],
  ['cpu_insn', 5063570],
  ['mem_byte', 1690992],
  ['invoke_time_nsecs', 757843],
  ['max_rw_key_byte', 112],
  ['max_rw_data_byte', 224],
  ['max_rw_code_byte', 0],
  ['max_emit_event_byte', 244],
];

describe('resource counters (#378)', () => {
  it('reads only core_metrics, ignoring the rest of the diagnostic stream', () => {
    const counters = readResourceCounters([
      fnCall(),
      counter('cpu_insn', 5063570),
      fnCall(),
    ]);
    expect([...counters]).toEqual([['cpu_insn', 5063570]]);
  });

  it('exposes every counter, in the host emission order', () => {
    const all = allResourceFacts(
      readResourceCounters(REAL.map(([n, v]) => counter(n, v)))
    );
    expect(all).toHaveLength(19);
    // Emission order preserved — the host's own ordering is the only one
    // these have, and re-sorting would invent a hierarchy.
    expect(all[0]).toEqual({ label: 'read_entry', value: '8' });
    expect(all[12]).toEqual({ label: 'cpu_insn', value: '5,063,570' });
    expect(all.at(-1)).toEqual({ label: 'max_emit_event_byte', value: '244' });
  });

  it('reports nothing for a classic transaction, which emits no counters', () => {
    // Classic operations emit no diagnostics at all (CAP-67), so the
    // disclosure must not render an empty shell.
    expect(allResourceFacts(readResourceCounters([]))).toEqual([]);
    expect(allResourceFacts(readResourceCounters([fnCall()]))).toEqual([]);
  });
});
