import type { EntityType, TruncationConfig } from './types.js';

/** Single-glyph ellipsis (`…`) — the canonical truncation marker. */
export const ELLIPSIS_CHAR = '…';

/**
 * The single truncation standard: first 4 + last 4 (`GCSO…DB2Z`).
 * Locked decision (task 0348 finding 13) — one standard for every identifier
 * reference, no per-type variants. Ledger is the sole exception (see below).
 */
export const DEFAULT_TRUNCATION: TruncationConfig = { prefix: 4, suffix: 4 };

/** Never-truncate config — ledger ids are plain sequence numbers, not hashes. */
const NO_TRUNCATION: TruncationConfig = { prefix: 0, suffix: 0 };

export function getDefaultTruncation(type: EntityType): TruncationConfig {
  return type === 'ledger' ? NO_TRUNCATION : DEFAULT_TRUNCATION;
}

export function truncateMiddle(
  value: string,
  config: TruncationConfig,
  ellipsis: string = ELLIPSIS_CHAR
): string {
  const { prefix, suffix } = config;
  if (prefix <= 0 && suffix <= 0) return value;
  if (value.length <= prefix + suffix + ellipsis.length) return value;
  return `${value.slice(0, prefix)}${ellipsis}${value.slice(-suffix)}`;
}
