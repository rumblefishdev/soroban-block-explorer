export type EntityType =
  | 'transaction'
  | 'account'
  | 'contract'
  | 'asset'
  | 'pool'
  | 'ledger'
  | 'nft';

export interface TruncationConfig {
  prefix: number;
  suffix: number;
}
