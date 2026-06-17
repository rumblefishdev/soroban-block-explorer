import type { OperationType } from '@rumblefish/api-types';

import { capitalize } from '../../utils/text.js';

export interface OperationTypeOption {
  /** Raw XDR operation-type enum, sent as `filter[operation_type]`. */
  value: OperationType;
  /** Human-readable label shown in the dropdown. */
  label: string;
}

/**
 * Human-readable label for every backend `OperationType` variant.
 *
 * Keyed as an exhaustive `Record<OperationType, string>` against the
 * generated union (`@rumblefish/api-types`, sourced from
 * `crates/domain/src/enums/operation_type.rs` via the OpenAPI spec). This
 * map is the single source of truth for both the filter dropdown and the
 * table pills. The exhaustiveness is the point: if the backend adds or
 * renames a variant, the regenerated `OperationType` union changes and
 * **this object fails to type-check** until the label is updated — closing
 * the silent-drift gap that a hand-maintained parallel list left open
 * (audit F-Z-2 / lore-0280).
 *
 * Declaration order is XDR discriminant order (matching the enum), and JS
 * preserves insertion order for these string keys, so `OPERATION_TYPE_OPTIONS`
 * below renders the dropdown in the canonical Stellar Laboratory / Horizon
 * ordering.
 *
 * Raw keys are SCREAMING_SNAKE_CASE — the case-sensitive form the backend
 * `filter[operation_type]` accepts. `TransactionsListPage` normalises
 * lowercase URL params to upper-case before passing through.
 */
const DISPLAY_LABELS: Record<OperationType, string> = {
  CREATE_ACCOUNT: 'Create Account',
  PAYMENT: 'Payment',
  PATH_PAYMENT_STRICT_RECEIVE: 'Path Payment Strict Receive',
  MANAGE_SELL_OFFER: 'Manage Sell Offer',
  CREATE_PASSIVE_SELL_OFFER: 'Create Passive Sell Offer',
  SET_OPTIONS: 'Set Options',
  CHANGE_TRUST: 'Change Trust',
  ALLOW_TRUST: 'Allow Trust',
  ACCOUNT_MERGE: 'Account Merge',
  INFLATION: 'Inflation',
  MANAGE_DATA: 'Manage Data',
  BUMP_SEQUENCE: 'Bump Sequence',
  MANAGE_BUY_OFFER: 'Manage Buy Offer',
  PATH_PAYMENT_STRICT_SEND: 'Path Payment Strict Send',
  CREATE_CLAIMABLE_BALANCE: 'Create Claimable Balance',
  CLAIM_CLAIMABLE_BALANCE: 'Claim Claimable Balance',
  BEGIN_SPONSORING_FUTURE_RESERVES: 'Begin Sponsoring Future Reserves',
  END_SPONSORING_FUTURE_RESERVES: 'End Sponsoring Future Reserves',
  REVOKE_SPONSORSHIP: 'Revoke Sponsorship',
  CLAWBACK: 'Clawback',
  CLAWBACK_CLAIMABLE_BALANCE: 'Clawback Claimable Balance',
  SET_TRUST_LINE_FLAGS: 'Set Trust Line Flags',
  LIQUIDITY_POOL_DEPOSIT: 'Liquidity Pool Deposit',
  LIQUIDITY_POOL_WITHDRAW: 'Liquidity Pool Withdraw',
  INVOKE_HOST_FUNCTION: 'Invoke Contract',
  EXTEND_FOOTPRINT_TTL: 'Extend Footprint TTL',
  RESTORE_FOOTPRINT: 'Restore Footprint',
};

/**
 * Operation types offered in the Transactions filter dropdown — full
 * parity with the backend `OperationType` enum, derived from
 * `DISPLAY_LABELS` so the dropdown and the table pills cannot diverge.
 */
export const OPERATION_TYPE_OPTIONS: readonly OperationTypeOption[] = (
  Object.entries(DISPLAY_LABELS) as [OperationType, string][]
).map(([value, label]) => ({ value, label }));

/** Pre-computed set for O(1) validation in `normalizeOperationType`. */
const VALID_OPS = new Set<string>(Object.keys(DISPLAY_LABELS));

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
  const known = (DISPLAY_LABELS as Record<string, string>)[raw];
  if (known) return known;
  return raw
    .toLowerCase()
    .split('_')
    .map((word) => capitalize(word))
    .join(' ');
}
