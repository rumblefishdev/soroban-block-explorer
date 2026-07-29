import type { OperationItem, XdrOperationDto } from '@rumblefish/api-types';

import { assetUnit, detailsObj } from '../normal/humanizeOp.js';

export interface OpFact {
  label: string;
  value: string;
}

/** Key-fact rows for the operation card — only where they ADD over the
 *  headline sentence. Today that is the path-payment route (finding #3 of the
 *  0359 audit) and the deliberately empty "Received" slot (spec D9: the exact
 *  delivered amount of a strict-send swap is not derivable from claimedAtoms;
 *  the slot lights up when the net_settled read path lands). */
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

  const facts: OpFact[] = [];
  const sendUnit = assetUnit(details.sendAsset, null);
  const destUnit = assetUnit(details.destAsset, null);
  const path = Array.isArray(details.path) ? details.path : [];
  if (sendUnit != null && destUnit != null) {
    const hops = path
      .map((asset) => assetUnit(asset, null))
      .filter((code): code is string => code != null);
    facts.push({
      label: 'Route',
      value: [sendUnit, ...hops, destUnit].join(' → '),
    });
  }
  const poolIds = Array.isArray(details.poolIds) ? details.poolIds : null;
  if (poolIds != null && poolIds.length > 0) {
    facts.push({
      label: 'Pools crossed',
      value: String(poolIds.length),
    });
  }
  if (light.type_name === 'PATH_PAYMENT_STRICT_SEND') {
    facts.push({ label: 'Received', value: '—' });
  }
  return facts;
}
