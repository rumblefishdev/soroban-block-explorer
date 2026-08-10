import type { E3ResponseTransactionDetailLight } from '@rumblefish/api-types';

const SPONSORSHIP = new Set([
  'BEGIN_SPONSORING_FUTURE_RESERVES',
  'END_SPONSORING_FUTURE_RESERVES',
]);

/** One-phrase classification of the whole transaction (spec D12, the
 *  stellarchain/Blockscout "story chip" pattern) — heuristic over operation
 *  types only. Returns null whenever unsure: an absent chip is fine, a wrong
 *  one is not. */
export function classifyTx(
  tx: E3ResponseTransactionDetailLight
): string | null {
  const types = (tx.operations ?? []).map((op) => op.type_name);
  if (types.length === 0) return null;
  const count = tx.operation_count;
  const suffix = count > 1 ? ` · ${count} ops` : '';
  const only = (pred: (t: string) => boolean) => types.every(pred);

  if (types.includes('INVOKE_HOST_FUNCTION')) return 'Contract call';
  if (only((t) => t === 'EXTEND_FOOTPRINT_TTL' || t === 'RESTORE_FOOTPRINT')) {
    return 'Contract maintenance';
  }
  const isPathPayment = (t: string) =>
    t === 'PATH_PAYMENT_STRICT_SEND' || t === 'PATH_PAYMENT_STRICT_RECEIVE';
  if (types.some(isPathPayment)) {
    // Trustline open/close around swaps is one motion, not a mixed bag.
    return only((t) => isPathPayment(t) || t === 'CHANGE_TRUST')
      ? `Swap${suffix}`
      : null;
  }
  if (only((t) => t === 'PAYMENT')) {
    return count > 1 ? `Payments · ${count}` : 'Payment';
  }
  if (
    only(
      (t) =>
        t === 'MANAGE_SELL_OFFER' ||
        t === 'MANAGE_BUY_OFFER' ||
        t === 'CREATE_PASSIVE_SELL_OFFER'
    )
  ) {
    return `Trading${suffix}`;
  }
  if (only((t) => t === 'CHANGE_TRUST')) return `Trustline update${suffix}`;
  // Sponsored onboarding (begin-sponsoring · create-account · end-sponsoring)
  // is account creation as one motion; anything else mixed stays unlabelled.
  if (
    types.includes('CREATE_ACCOUNT') &&
    only((t) => t === 'CREATE_ACCOUNT' || SPONSORSHIP.has(t))
  ) {
    return `Account creation${suffix}`;
  }
  return null;
}
