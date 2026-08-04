import type { XdrEventDto } from '@rumblefish/api-types';
import { describe, expect, it } from 'vitest';

import { readResourceCounters, resourceSummary } from './resources.js';

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

  it('summarises the real mainnet set to five grouped facts', () => {
    const facts = resourceSummary(
      readResourceCounters(REAL.map(([n, v]) => counter(n, v)))
    );
    expect(facts).toEqual([
      { label: 'Instructions', value: '5,063,570' },
      { label: 'Memory', value: '1,690,992 B' },
      { label: 'Ledger I/O', value: '116 B read · 412 B written' },
      { label: 'Entries', value: '8 read · 3 written' },
      { label: 'Time', value: '757,843 ns' },
    ]);
  });

  it('reports nothing for a classic transaction, which emits no counters', () => {
    // Classic operations emit no diagnostics at all (CAP-67), so the strip
    // must not render an empty shell.
    expect(resourceSummary(readResourceCounters([]))).toEqual([]);
    expect(resourceSummary(readResourceCounters([fnCall()]))).toEqual([]);
  });

  it('omits a fact whose counter is missing rather than showing a blank', () => {
    const facts = resourceSummary(
      readResourceCounters([counter('cpu_insn', 42)])
    );
    expect(facts).toEqual([{ label: 'Instructions', value: '42' }]);
  });
});
