export interface OperationTypeOption {
  /** Raw XDR operation-type enum, sent as `filter[operation_type]`. */
  value: string;
  /** Human-readable label shown in the dropdown. */
  label: string;
}

/**
 * Operation types offered in the Transactions filter dropdown — full
 * parity with the backend `OperationType` enum
 * (`crates/domain/src/enums/operation_type.rs`, Protocol 21, 27
 * variants). Listed in XDR discriminant order so the dropdown matches
 * the canonical ordering used by Stellar Laboratory / Horizon.
 *
 * Raw values are SCREAMING_SNAKE_CASE — the case-sensitive form the
 * backend `filter[operation_type]` accepts. `TransactionsListPage`
 * normalises lowercase URL params to upper-case before passing through.
 */
export const OPERATION_TYPE_OPTIONS: readonly OperationTypeOption[] = [
  { value: 'CREATE_ACCOUNT', label: 'Create Account' },
  { value: 'PAYMENT', label: 'Payment' },
  {
    value: 'PATH_PAYMENT_STRICT_RECEIVE',
    label: 'Path Payment Strict Receive',
  },
  { value: 'MANAGE_SELL_OFFER', label: 'Manage Sell Offer' },
  { value: 'CREATE_PASSIVE_SELL_OFFER', label: 'Create Passive Sell Offer' },
  { value: 'SET_OPTIONS', label: 'Set Options' },
  { value: 'CHANGE_TRUST', label: 'Change Trust' },
  { value: 'ALLOW_TRUST', label: 'Allow Trust' },
  { value: 'ACCOUNT_MERGE', label: 'Account Merge' },
  { value: 'INFLATION', label: 'Inflation' },
  { value: 'MANAGE_DATA', label: 'Manage Data' },
  { value: 'BUMP_SEQUENCE', label: 'Bump Sequence' },
  { value: 'MANAGE_BUY_OFFER', label: 'Manage Buy Offer' },
  { value: 'PATH_PAYMENT_STRICT_SEND', label: 'Path Payment Strict Send' },
  { value: 'CREATE_CLAIMABLE_BALANCE', label: 'Create Claimable Balance' },
  { value: 'CLAIM_CLAIMABLE_BALANCE', label: 'Claim Claimable Balance' },
  {
    value: 'BEGIN_SPONSORING_FUTURE_RESERVES',
    label: 'Begin Sponsoring Future Reserves',
  },
  {
    value: 'END_SPONSORING_FUTURE_RESERVES',
    label: 'End Sponsoring Future Reserves',
  },
  { value: 'REVOKE_SPONSORSHIP', label: 'Revoke Sponsorship' },
  { value: 'CLAWBACK', label: 'Clawback' },
  { value: 'CLAWBACK_CLAIMABLE_BALANCE', label: 'Clawback Claimable Balance' },
  { value: 'SET_TRUST_LINE_FLAGS', label: 'Set Trust Line Flags' },
  { value: 'LIQUIDITY_POOL_DEPOSIT', label: 'Liquidity Pool Deposit' },
  { value: 'LIQUIDITY_POOL_WITHDRAW', label: 'Liquidity Pool Withdraw' },
  { value: 'INVOKE_HOST_FUNCTION', label: 'Invoke Contract' },
  { value: 'EXTEND_FOOTPRINT_TTL', label: 'Extend Footprint TTL' },
  { value: 'RESTORE_FOOTPRINT', label: 'Restore Footprint' },
];

/**
 * Display labels for operation-type pills in the table, derived from
 * `OPERATION_TYPE_OPTIONS` so the dropdown and the table pills stay in
 * sync. Unmapped values fall back to a title-cased enum name via
 * `formatOperationType` below.
 */
const DISPLAY_LABELS: Record<string, string> = Object.fromEntries(
  OPERATION_TYPE_OPTIONS.map((opt) => [opt.value, opt.label])
);

/** Pre-computed set for O(1) validation in `normalizeOperationType`. */
const VALID_OPS = new Set(OPERATION_TYPE_OPTIONS.map((opt) => opt.value));

/**
 * Normalises a raw `op` URL param to the case-sensitive backend enum
 * form, or an empty string if the value is missing / unknown.
 *
 * The backend `OperationType` enum is `SCREAMING_SNAKE_CASE` and rejects
 * anything else with a 400. Lowercase URLs (`?op=manage_buy_offer`) —
 * from copied links, hand-typed URLs, or stale bookmarks — would
 * otherwise blow up the whole transactions page with a generic error
 * banner. Trim + upper + allow-list against the known enum: invalid
 * values silently drop the filter rather than failing the request.
 */
export function normalizeOperationType(raw: string | null | undefined): string {
  if (!raw) return '';
  const up = raw.trim().toUpperCase();
  return VALID_OPS.has(up) ? up : '';
}

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
