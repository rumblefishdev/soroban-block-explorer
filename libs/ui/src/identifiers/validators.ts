const HEX_64 = /^[0-9a-fA-F]{64}$/;
const STELLAR_ACCOUNT = /^G[A-Z2-7]{55}$/;
const STELLAR_CONTRACT = /^C[A-Z2-7]{55}$/;
const STELLAR_POOL = /^L[A-Z2-7]{55}$/;
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

export function isPoolId(value: string): boolean {
  return STELLAR_POOL.test(value);
}
