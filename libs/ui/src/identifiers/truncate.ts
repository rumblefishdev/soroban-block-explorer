import type { EntityType, TruncationConfig } from './types.js';

const ELLIPSIS = '...';

// Prefix/suffix lengths verified against the Figma tables (home + the
// Transactions list page both truncate tx hashes 6/4 and accounts 4/4).
const defaultByType: Record<EntityType, TruncationConfig> = {
  transaction: { prefix: 6, suffix: 4 },
  account: { prefix: 4, suffix: 4 },
  contract: { prefix: 6, suffix: 4 },
  asset: { prefix: 6, suffix: 4 },
  pool: { prefix: 6, suffix: 4 },
  nft: { prefix: 6, suffix: 4 },
  ledger: { prefix: 0, suffix: 0 },
};

export function getDefaultTruncation(type: EntityType): TruncationConfig {
  return defaultByType[type];
}

export function truncateMiddle(
  value: string,
  config: TruncationConfig
): string {
  const { prefix, suffix } = config;
  if (prefix <= 0 && suffix <= 0) return value;
  if (value.length <= prefix + suffix + ELLIPSIS.length) return value;
  return `${value.slice(0, prefix)}${ELLIPSIS}${value.slice(-suffix)}`;
}
