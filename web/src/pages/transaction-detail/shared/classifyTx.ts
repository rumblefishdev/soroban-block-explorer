import type { E3ResponseTransactionDetailLight } from '@rumblefish/api-types';

/** One-phrase classification of the whole transaction (spec D12, the
 *  stellarchain/Blockscout "story chip" pattern) — heuristic over operation
 *  types only. Returns null whenever unsure: an absent chip is fine, a wrong
 *  one is not. */
export function classifyTx(
  tx: E3ResponseTransactionDetailLight
): string | null {
  const types = (tx.operations ?? []).map((op) => op.type_name);
  if (types.length === 0) return null;
  const unique = new Set(types);
  const count = tx.operation_count;
  const suffix = count > 1 ? ` · ${count} ops` : '';

  if (unique.has('INVOKE_HOST_FUNCTION')) return 'Contract call';
  if (
    [...unique].every(
      (t) => t === 'EXTEND_FOOTPRINT_TTL' || t === 'RESTORE_FOOTPRINT'
    )
  ) {
    return 'Contract maintenance';
  }
  const isPathPayment = (t: string) =>
    t === 'PATH_PAYMENT_STRICT_SEND' || t === 'PATH_PAYMENT_STRICT_RECEIVE';
  if (types.some(isPathPayment)) {
    // Trustline open/close around swaps is one motion, not a mixed bag.
    const rest = [...unique].filter(
      (t) => !isPathPayment(t) && t !== 'CHANGE_TRUST'
    );
    return rest.length === 0 ? `Swap${suffix}` : null;
  }
  if ([...unique].every((t) => t === 'PAYMENT')) {
    return count > 1 ? `Payments · ${count}` : 'Payment';
  }
  if (
    [...unique].every(
      (t) =>
        t === 'MANAGE_SELL_OFFER' ||
        t === 'MANAGE_BUY_OFFER' ||
        t === 'CREATE_PASSIVE_SELL_OFFER'
    )
  ) {
    return `Trading${suffix}`;
  }
  if ([...unique].every((t) => t === 'CHANGE_TRUST')) {
    return `Trustline update${suffix}`;
  }
  if (unique.has('CREATE_ACCOUNT')) return `Account creation${suffix}`;
  return null;
}
