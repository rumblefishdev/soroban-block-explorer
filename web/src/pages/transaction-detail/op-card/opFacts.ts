import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';

import { detailsObj } from '../shared/humanizeOp.js';

export interface OpFact {
  label: string;
  value: string;
}

/** Key-fact rows for the operation card — only where they ADD over the
 *  headline sentence and the RouteStrip (which owns the route chain). Today:
 *  the crossed-pool count and the deliberately empty "Received" slot (spec
 *  D9: the exact delivered amount of a strict-send swap is not derivable from
 *  claimedAtoms; the slot lights up when the net_settled read path lands). */
export function opFacts(
  light: OperationItem,
  heavy: XdrOperationDto | null
): OpFact[] {
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
    facts.push({ label: 'Received', value: '—' });
  }
  return facts;
}
