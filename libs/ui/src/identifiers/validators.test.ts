import { describe, expect, it } from 'vitest';

import {
  isAccountId,
  isContractId,
  isLedgerSequence,
  isPoolId,
  isTransactionHash,
  isValidIdentifier,
} from './validators.js';

const VALID_ACCOUNT =
  'GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOUJ3K4MOMNGEE36';
const VALID_CONTRACT =
  'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC';
const VALID_POOL = 'LBJRKPHNDQX6FMT2BIPW5ELSZAHOV4DKRY7GNU3CJQX6FMT2BIPW5ELS';
const VALID_TX =
  '7b2a8c1f9d4e6a3b8c1f9d4e6a3b8c1f9d4e6a3b8c1f9d4e6a3b8c1f9d4e6a3b';

describe('isAccountId', () => {
  it('accepts a valid CAP-23 G-strkey', () => {
    expect(isAccountId(VALID_ACCOUNT)).toBe(true);
  });

  it('rejects strkeys with the wrong prefix', () => {
    expect(isAccountId(VALID_CONTRACT)).toBe(false);
    expect(isAccountId(VALID_POOL)).toBe(false);
  });

  it('rejects malformed or wrong-length strings', () => {
    expect(isAccountId('')).toBe(false);
    expect(isAccountId('GDQP2KPQ')).toBe(false); // too short
    expect(isAccountId(VALID_ACCOUNT.slice(0, -1))).toBe(false); // off-by-one
    expect(isAccountId(`${VALID_ACCOUNT}A`)).toBe(false); // too long
  });

  it('rejects characters outside the base32 alphabet', () => {
    expect(isAccountId('G' + '0'.repeat(55))).toBe(false); // 0 not in [A-Z2-7]
    expect(isAccountId('G' + '1'.repeat(55))).toBe(false); // 1 not in [A-Z2-7]
    expect(isAccountId('g' + 'a'.repeat(55))).toBe(false); // lowercase
  });
});

describe('isContractId', () => {
  it('accepts a valid C-strkey', () => {
    expect(isContractId(VALID_CONTRACT)).toBe(true);
  });
  it('rejects accounts, pools, and arbitrary strings', () => {
    expect(isContractId(VALID_ACCOUNT)).toBe(false);
    expect(isContractId(VALID_POOL)).toBe(false);
    expect(isContractId('')).toBe(false);
  });
});

describe('isPoolId', () => {
  it('accepts a valid CAP-38 L-strkey', () => {
    expect(isPoolId(VALID_POOL)).toBe(true);
  });
  it('rejects accounts and contracts', () => {
    expect(isPoolId(VALID_ACCOUNT)).toBe(false);
    expect(isPoolId(VALID_CONTRACT)).toBe(false);
  });
});

describe('isTransactionHash', () => {
  it('accepts a 64-character hex string regardless of case', () => {
    expect(isTransactionHash(VALID_TX)).toBe(true);
    expect(isTransactionHash(VALID_TX.toUpperCase())).toBe(true);
  });
  it('rejects strings that are not 64 hex chars', () => {
    expect(isTransactionHash('')).toBe(false);
    expect(isTransactionHash('abc')).toBe(false);
    expect(isTransactionHash(VALID_TX.slice(0, 63))).toBe(false);
    expect(isTransactionHash(`${VALID_TX}0`)).toBe(false);
    expect(isTransactionHash(`g${VALID_TX.slice(1)}`)).toBe(false); // g is not hex
  });
});

describe('isLedgerSequence', () => {
  it('accepts positive integers (string or number)', () => {
    expect(isLedgerSequence(1)).toBe(true);
    expect(isLedgerSequence('54837201')).toBe(true);
  });
  it('rejects zero, negatives, decimals, and non-numeric strings', () => {
    expect(isLedgerSequence(0)).toBe(false);
    expect(isLedgerSequence('-1')).toBe(false);
    expect(isLedgerSequence('1.5')).toBe(false);
    expect(isLedgerSequence('abc')).toBe(false);
    expect(isLedgerSequence('')).toBe(false);
  });
});

describe('isValidIdentifier', () => {
  it('dispatches to the per-type validator', () => {
    expect(isValidIdentifier('account', VALID_ACCOUNT)).toBe(true);
    expect(isValidIdentifier('contract', VALID_CONTRACT)).toBe(true);
    expect(isValidIdentifier('pool', VALID_POOL)).toBe(true);
    expect(isValidIdentifier('transaction', VALID_TX)).toBe(true);
    expect(isValidIdentifier('ledger', '12345')).toBe(true);
  });

  it('asset/nft pass through with any non-empty value (composite ids)', () => {
    expect(isValidIdentifier('asset', 'USDC-GA5Z')).toBe(true);
    expect(isValidIdentifier('asset', '')).toBe(false);
    expect(isValidIdentifier('nft', '123')).toBe(true);
    expect(isValidIdentifier('nft', '')).toBe(false);
  });
});
