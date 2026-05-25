import type { EntityType } from './types.js';

const HEX_64 = /^[0-9a-fA-F]{64}$/;
const HEX_64_LOWER = /^[0-9a-f]{64}$/;
const STELLAR_ACCOUNT = /^G[A-Z2-7]{55}$/;
const STELLAR_CONTRACT = /^C[A-Z2-7]{55}$/;
const POSITIVE_INT = /^\d+$/;

export function isTransactionHash(value: string): boolean {
  return HEX_64.test(value);
}

export function isAccountId(value: string): boolean {
  return STELLAR_ACCOUNT.test(value);
}

export function isContractId(value: string): boolean {
  return STELLAR_CONTRACT.test(value);
}

export function isLedgerSequence(value: string | number): boolean {
  const s = String(value);
  return POSITIVE_INT.test(s) && Number(s) > 0;
}

/**
 * Backend serves liquidity-pool ids as a 64-char lowercase hex
 * representation of the SHA-256 pool hash. The strkey encoder
 * (`poolIdHexToStrkey`) requires this shape and throws otherwise,
 * which would crash the detail-page header synchronously on mount.
 * Validate up front so an invalid id renders `NotFoundState` instead
 * of falling into a generic-error banner. Tolerates upper-case input
 * for resilient deep-linking.
 */
export function isPoolId(value: string): boolean {
  return HEX_64_LOWER.test(value.toLowerCase());
}

export function isValidIdentifier(type: EntityType, value: string): boolean {
  switch (type) {
    case 'transaction':
      return isTransactionHash(value);
    case 'account':
      return isAccountId(value);
    case 'contract':
      return isContractId(value);
    case 'ledger':
      return isLedgerSequence(value);
    case 'pool':
      return isPoolId(value);
    case 'asset':
    case 'nft':
      return value.length > 0;
  }
}
