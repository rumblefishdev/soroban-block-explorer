import type {
  E3ResponseTransactionDetailLight,
  OperationItem,
  XdrOperationDto,
} from '@rumblefish/api-types';

/**
 * One pickable operation: the row the picker renders plus the light/heavy
 * pair the detail panels read.
 *
 * `tx.operations` (light) folds same-identity envelope ops into a single row
 * (task 0163), so it under-counts multi-op transactions. `tx.heavy.operations`
 * is the XDR-decoded, unfolded 1:1 list — when present we drive the picker
 * from it so every operation shows; otherwise we fall back to the folded
 * light rows (task 0329).
 */
export interface OperationEntry {
  row: OperationItem;
  light: OperationItem | undefined;
  heavy: XdrOperationDto | null;
}

function matchLight(
  heavy: XdrOperationDto,
  lightOps: readonly OperationItem[]
): OperationItem | undefined {
  const upper = heavy.op_type.toUpperCase();
  return (
    lightOps.find((l) => l.application_order === heavy.application_order) ??
    // ponytail: folded rows share one light identity per type, so a type
    // match returns the right identity for every heavy op in the fold. Mixed
    // same-type folds with differing destinations would pick the first row —
    // acceptable, since identity fields are shared within a fold anyway.
    lightOps.find((l) => l.type_name === upper)
  );
}

export function buildOperationEntries(
  tx: E3ResponseTransactionDetailLight
): OperationEntry[] {
  const lightOps = tx.operations;
  const heavyOps = tx.heavy?.operations ?? [];

  // Both causes of an empty `heavyOps` land here — no heavy block at all, or a
  // heavy block whose operations came back empty — and both get the same folded
  // fallback. They are NOT distinguished, deliberately: the caller decides what
  // to say about it, since only it knows the header count to compare against
  // (`OperationsSection` warns whenever the archive data is missing).
  if (heavyOps.length === 0) {
    return lightOps.map((light) => ({ row: light, light, heavy: null }));
  }

  return heavyOps.map((heavy) => {
    const light = matchLight(heavy, lightOps);
    const base: OperationItem = light ?? {
      appearance_id: heavy.application_order,
      created_at: tx.created_at,
      ledger_sequence: tx.ledger_sequence,
      pool_ids: [],
      type: 0,
      type_name: heavy.op_type.toUpperCase(),
    };
    // appearance_id keys the picker list; override so folded entries sharing
    // one light row still get unique, stable keys (the real appearance_id
    // lives on `light`, which the panels use).
    const row: OperationItem = {
      ...base,
      appearance_id: heavy.application_order,
      application_order: heavy.application_order,
    };
    return { row, light, heavy };
  });
}
