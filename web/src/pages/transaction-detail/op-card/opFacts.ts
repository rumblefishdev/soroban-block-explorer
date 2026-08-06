import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';

import { detailsObj } from '../shared/humanizeOp.js';

export interface OpFact {
  label: string;
  value: string;
}

/** Key-fact rows for the operation card — only where they ADD over the
 *  headline sentence and the RouteStrip (which owns the route chain). Today
 *  just the "Received" slot for an APPLIED strict-send swap (spec D9: the exact
 *  delivered amount is not derivable from claimedAtoms; the slot lights up when
 *  the net_settled read path lands).
 *
 *  `applied` is required, not optional: on a FAILED path payment nothing was
 *  delivered, so the amount is known to be zero rather than unknown, and
 *  claiming "not derivable" there would assert an unknown over a fact — the
 *  inverse of the defect this row was reworded to avoid (0377 F7). */
export function opFacts(
  light: OperationItem,
  heavy: XdrOperationDto | null,
  applied: boolean
): OpFact[] {
  if (!applied) return [];
  if (
    light.type_name !== 'PATH_PAYMENT_STRICT_SEND' &&
    light.type_name !== 'PATH_PAYMENT_STRICT_RECEIVE'
  ) {
    return [];
  }
  const details = detailsObj(heavy);
  if (details == null) return [];

  // Crossed pools render as links in the card (task 0305, absorbed here) —
  // no count fact needed.
  const facts: OpFact[] = [];
  if (light.type_name === 'PATH_PAYMENT_STRICT_SEND') {
    // The delivered amount exists on-chain but is not derivable from the
    // claimed atoms we hold, so it is unknown — not zero. A bare "—" on a
    // SUCCESSFUL swap reads as "received nothing" (0377 F7).
    facts.push({ label: 'Received', value: 'not derivable' });
  }
  return facts;
}
