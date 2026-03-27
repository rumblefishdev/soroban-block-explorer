export interface LedgerPointer {
  sequence: number;
  closedAt: string;
}

export interface TransactionPointer {
  hash: string;
  ledgerSequence: number;
}

// --- Shared type primitives ---

export type JsonValue =
  | string
  | number
  | boolean
  | null
  | readonly JsonValue[]
  | { readonly [key: string]: JsonValue };

/** Decoded Soroban ScVal. Placeholder until task 0013 provides a full ScVal type. */
export type ScVal = JsonValue;

/** String representation of a PostgreSQL BIGINT/BIGSERIAL value. */
export type BigIntString = string;

/** String representation of a PostgreSQL NUMERIC / DECIMAL value. */
export type NumericString = string;

// --- Soroban domain types ---

export type ContractType = 'token' | 'dex' | 'lending' | 'nft' | 'other';

export interface ContractFunction {
  name: string;
  parameters: readonly { name: string; type: string }[];
  returnType: string;
}

export interface ContractMetadata {
  functions?: readonly ContractFunction[];
  [key: string]: JsonValue | readonly ContractFunction[] | undefined;
}

export interface SorobanContract {
  contractId: string;
  wasmHash: string | null;
  deployerAccount: string | null;
  deployedAtLedger: BigIntString | null;
  contractType: ContractType | null;
  isSac: boolean | null;
  metadata: ContractMetadata | null;
}

export type EventType = 'contract' | 'system' | 'diagnostic';

export interface SorobanInvocation {
  id: BigIntString;
  transactionId: BigIntString | null;
  contractId: string | null;
  callerAccount: string | null;
  functionName: string;
  functionArgs: ScVal | null;
  returnValue: ScVal | null;
  successful: boolean;
  ledgerSequence: BigIntString;
  createdAt: string;
}

export interface SorobanEvent {
  id: BigIntString;
  transactionId: BigIntString | null;
  contractId: string | null;
  eventType: EventType;
  topics: readonly ScVal[];
  data: ScVal;
  ledgerSequence: BigIntString;
  createdAt: string;
}

export type InterpretationType = 'swap' | 'transfer' | 'mint' | 'burn';

export interface EventInterpretation {
  id: BigIntString;
  eventId: BigIntString | null;
  interpretationType: InterpretationType;
  humanReadable: string;
  structuredData: Readonly<Record<string, JsonValue>>;
}

// --- Token, Account, NFT domain types ---

export type AssetType = 'classic' | 'sac' | 'soroban';

/**
 * Explorer token — unifies classic Stellar assets and Soroban-native tokens.
 *
 * Identity varies by asset type:
 * - classic: `assetCode` + `issuerAddress` (UNIQUE constraint)
 * - sac/soroban: `contractId` (UNIQUE constraint, FK → soroban_contracts)
 */
export interface Token {
  id: number;
  assetType: AssetType;
  assetCode: string | null;
  issuerAddress: string | null;
  contractId: string | null;
  name: string | null;
  totalSupply: NumericString | null;
  /** DDL: INT DEFAULT 0. Semantically non-nullable — always initialized to 0. */
  holderCount: number;
  metadata: JsonValue | null;
}

/**
 * Explorer account — derived-state entity with ledger-sequence watermarks.
 *
 * `firstSeenLedger` / `lastSeenLedger` (FK → ledgers.sequence) enforce
 * monotonic updates: older batches cannot overwrite newer state.
 */
export interface Account {
  accountId: string;
  firstSeenLedger: BigIntString;
  lastSeenLedger: BigIntString;
  sequenceNumber: BigIntString | null;
  balances: readonly JsonValue[];
  homeDomain: string | null;
}

/**
 * Explorer NFT — token scoped by contract.
 *
 * UNIQUE(contractId, tokenId). Transfer history is derived from stored
 * Soroban events, not a separate table. Many fields are nullable because
 * NFT contract conventions are not standardized.
 */
export interface NFT {
  id: BigIntString;
  contractId: string;
  tokenId: string;
  collectionName: string | null;
  ownerAccount: string | null;
  name: string | null;
  mediaUrl: string | null;
  metadata: JsonValue | null;
  mintedAtLedger: BigIntString | null;
  lastSeenLedger: BigIntString | null;
}
