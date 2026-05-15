export interface OperationTypeOption {
  /** Raw XDR operation-type enum, sent as `filter[operation_type]`. */
  value: string;
  /** Human-readable label shown in the dropdown. */
  label: string;
}

/**
 * Operation types offered in the Transactions filter dropdown — exactly the
 * set shown in the Figma design. The raw values are Stellar XDR
 * `OperationType` enum names, matching what the API accepts in
 * `filter[operation_type]`.
 *
 * Note: "Path Payment" and "Manage Offer" are simplified labels — each maps
 * to the more common of two XDR variants (strict-receive / sell-offer).
 */
export const OPERATION_TYPE_OPTIONS: readonly OperationTypeOption[] = [
  { value: 'PATH_PAYMENT_STRICT_RECEIVE', label: 'Path Payment' },
  { value: 'INVOKE_HOST_FUNCTION', label: 'Invoke Contract' },
  { value: 'CREATE_ACCOUNT', label: 'Create Account' },
  { value: 'PAYMENT', label: 'Payment' },
  { value: 'MANAGE_SELL_OFFER', label: 'Manage Offer' },
];

/**
 * Display labels for operation-type pills in the table. The table renders any
 * operation type the API returns, so this covers the types visible in the
 * Figma table design; unmapped types fall back to title-cased enum names.
 */
const DISPLAY_LABELS: Record<string, string> = {
  PATH_PAYMENT_STRICT_RECEIVE: 'Path Payment',
  INVOKE_HOST_FUNCTION: 'Invoke Contract',
  CREATE_ACCOUNT: 'Create Account',
  PAYMENT: 'Payment',
  MANAGE_SELL_OFFER: 'Manage Offer',
  CHANGE_TRUST: 'Change Trust',
  LIQUIDITY_POOL_DEPOSIT: 'Liquidity Pool Deposit',
};

/**
 * Human-readable label for any raw operation-type enum. Falls back to
 * title-casing the enum name for types without an explicit label.
 */
export function formatOperationType(raw: string): string {
  const known = DISPLAY_LABELS[raw];
  if (known) return known;
  return raw
    .toLowerCase()
    .split('_')
    .map((word) =>
      word.length > 0 ? word[0].toUpperCase() + word.slice(1) : word
    )
    .join(' ');
}
