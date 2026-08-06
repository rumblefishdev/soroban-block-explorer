import type {
  E3ResponseTransactionDetailLight,
  OperationItem,
  XdrOperationDto,
} from '@rumblefish/api-types';

/**
 * One pickable operation: the row the picker renders plus the light/heavy
 * pair the detail panels read.
 *
 * Driven ONLY by `tx.heavy.operations`, the XDR-decoded 1:1 list. When the
 * archive fetch fails this returns nothing and the section says so.
 *
 * There used to be a fallback to `tx.operations` (task 0329). It was dropped
 * deliberately: that list is the DB's appearance index, which folds
 * same-identity operations into one row (task 0163) WITHOUT amount in the key,
 * so four payments of different amounts between the same pair collapse to a
 * single row. Rendering it as the operation list showed "1" where the header
 * said "4", and no wording fixed that — the row is not one operation, it is a
 * count of them with the amounts gone. Showing nothing and saying why beats
 * showing a shape the user will read as the truth (0377 F7).
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
