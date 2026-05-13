export type EntityType =
  | 'transaction'
  | 'account'
  | 'contract'
  | 'token'
  | 'pool'
  | 'ledger'
  | 'nft';

export interface TruncationConfig {
  prefix: number;
  suffix: number;
}
