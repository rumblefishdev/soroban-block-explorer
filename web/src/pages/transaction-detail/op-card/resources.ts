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
 * Placement: behind a disclosure on the invoke operation card, beside the
 * execution trace. ALL of them or none — a curated five above the fold with
 * the same five repeated inside would be saying the same thing twice, and
 * picking a favourite handful is a hierarchy the protocol does not state.
 * The counters are per-transaction, but a Soroban transaction carries exactly
 * one invoke operation (protocol rule, confirmed across 1,376,903 mainnet
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
 * Every counter the host reported, in its own emission order — re-sorting
 * would invent a ranking the protocol never states. The audience is a
 * contract author tuning a footprint, and there `emit_event_byte` and the
 * `max_rw_*` ceilings matter as much as `cpu_insn`.
 */
export function allResourceFacts(
  counters: Map<string, number>
): ResourceFact[] {
  return [...counters].map(([label, value]) => ({
    label,
    value: formatInteger(value),
  }));
}
