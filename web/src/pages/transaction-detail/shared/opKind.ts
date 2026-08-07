/** The Soroban-side operation types — one definition so the card's kind chip
 *  and the details panel's category chip can never disagree. */
const SOROBAN_TYPES = new Set([
  'INVOKE_HOST_FUNCTION',
  'EXTEND_FOOTPRINT_TTL',
  'RESTORE_FOOTPRINT',
]);

export function isSorobanOp(typeName: string): boolean {
  return SOROBAN_TYPES.has(typeName.toUpperCase());
}
