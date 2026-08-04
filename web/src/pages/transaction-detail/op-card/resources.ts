import type { XdrEventDto } from '@rumblefish/api-types';
import { formatInteger } from '@rumblefish/soroban-block-explorer-ui';

import { symTopic } from './ExecutionTrace.js';

/**
 * The host's resource meter, lifted out of the diagnostic stream.
 *
 * Soroban emits these as `core_metrics` diagnostic events — one event per
 * counter, always the SAME nineteen counters, measured across nine mainnet
 * transactions from a 2-call transfer to a failed 98-call swap. That makes
 * them one record with nineteen fields, not nineteen events, and rendering
 * them as rows in an event table was a category error: they carry no order,
 * no nesting and no relation to what the contract did. They are the bill,
 * not the story.
 *
 * Other explorers agree — stellarchain shows them as a "Contract resources"
 * panel, stellar.expert not at all, and the protocol's own `getEvents` never
 * returns diagnostic events in the first place.
 *
 * Placement: the invoke operation card, beside the execution trace. The
 * counters are per-transaction, but a Soroban transaction carries exactly one
 * invoke operation (protocol rule, confirmed across 1,376,903 mainnet
 * transactions — never two), so per-transaction and per-operation are the
 * same thing here.
 */

/** Counter name → value, for every `core_metrics` event in the stream. */
export function readResourceCounters(
  events: readonly XdrEventDto[]
): Map<string, number> {
  const out = new Map<string, number>();
  for (const event of events) {
    if (symTopic(event, 0) !== 'core_metrics') continue;
    const name = symTopic(event, 1);
    const raw = (event.data as { value?: unknown } | null)?.value;
    // The decoder emits u64 as a JSON number. Every counter observed on
    // mainnet is far below 2^53, so no precision is lost; a value that ever
    // arrived as a string is skipped rather than coerced into a wrong number.
    if (name != null && typeof raw === 'number') out.set(name, raw);
  }
  return out;
}

export interface ResourceFact {
  label: string;
  /** Already grouped for display — never a bare number. */
  value: string;
}

/**
 * The five facts worth reading at a glance. The other fourteen counters are
 * breakdowns of these (`read_key_byte`, `write_data_byte`, the `max_rw_*`
 * ceilings…) and stay behind the full list — summary first, details on demand.
 *
 * Returns `[]` when the stream carried no counters, which is every classic
 * transaction: they emit no diagnostics at all (CAP-67).
 */
export function resourceSummary(counters: Map<string, number>): ResourceFact[] {
  if (counters.size === 0) return [];
  const n = (key: string) => counters.get(key);
  const facts: ResourceFact[] = [];
  const push = (label: string, value: string | null) => {
    if (value != null) facts.push({ label, value });
  };
  const int = (key: string, unit = '') => {
    const v = n(key);
    return v == null ? null : `${formatInteger(v)}${unit}`;
  };
  const pair = (readKey: string, writeKey: string, unit: string) => {
    const r = n(readKey);
    const w = n(writeKey);
    return r == null || w == null
      ? null
      : `${formatInteger(r)}${unit} read · ${formatInteger(w)}${unit} written`;
  };

  // Instructions lead: they are the largest component of the resource fee, so
  // this is the number that explains the charge on the summary above.
  push('Instructions', int('cpu_insn'));
  push('Memory', int('mem_byte', ' B'));
  push('Ledger I/O', pair('ledger_read_byte', 'ledger_write_byte', ' B'));
  push('Entries', pair('read_entry', 'write_entry', ''));
  push('Time', int('invoke_time_nsecs', ' ns'));
  return facts;
}
