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

/** Counter name → value, for EVERY `core_metrics` event in the stream.
 *
 *  Total by construction. This panel is the only place the counters render —
 *  the raw diagnostics table leaves them to it — so a counter dropped here is
 *  a counter deleted from the page, and "18 of 19, silently" is precisely the
 *  failure this codebase refuses. Nothing is skipped: an unexpected shape
 *  passes through verbatim and is displayed as it arrived.
 *
 *  The decoder emits u64 as a JSON number, and every counter observed on
 *  mainnet is far below 2^53, so the common path loses no precision. A value
 *  arriving as a decimal string (a big int) is kept as that string rather than
 *  coerced into a wrong number. */
export function readResourceCounters(
  events: readonly XdrEventDto[]
): Map<string, number | string> {
  const out = new Map<string, number | string>();
  for (const event of events) {
    if (symTopic(event, 0) !== 'core_metrics') continue;
    const name = symTopic(event, 1) ?? `(unnamed #${event.event_index})`;
    const raw = (event.data as { value?: unknown } | null)?.value;
    out.set(
      name,
      typeof raw === 'number' || typeof raw === 'string'
        ? raw
        : JSON.stringify(raw ?? null)
    );
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
  counters: Map<string, number | string>
): ResourceFact[] {
  return [...counters].map(([label, value]) => ({
    label,
    // Grouping is for numbers we decoded. Anything else shows as it arrived —
    // formatting an unparsed value would dress it up as something we read.
    value: typeof value === 'number' ? formatInteger(value) : value,
  }));
}
