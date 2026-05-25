/**
 * Hand-written types for the contract interface metadata blob.
 *
 * The generated API type leaves `InterfaceResponse.interface_metadata` as
 * `unknown` — it is a JSONB column. The real shape is produced by the
 * indexer (`crates/indexer/src/handler/persist/staging.rs` — the
 * `{ functions, wasm_byte_len }` object) from the contract's WASM spec.
 * SAC / pre-upload / stub contracts have `interface_metadata = null`.
 */

export interface ContractFunctionParam {
  name: string;
  type_name: string;
}

export interface ContractFunctionSig {
  name: string;
  doc: string;
  inputs: ContractFunctionParam[];
  outputs: string[];
}

export interface ContractInterfaceMetadata {
  functions: ContractFunctionSig[];
  wasm_byte_len: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  // Arrays are also `typeof === 'object'` — exclude them so a malformed
  // blob that puts an array where an object is expected does not silently
  // turn into a junk entry with an empty name.
  return value != null && typeof value === 'object' && !Array.isArray(value);
}

function asString(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback;
}

/**
 * Narrows the API's `unknown` `interface_metadata` to the known shape.
 * Returns `null` for SAC / pre-upload contracts (no metadata) and for any
 * blob that does not carry a `functions` array. Defensive by design — the
 * JSONB shape is owned by the indexer, not the OpenAPI contract.
 */
export function parseInterfaceMetadata(
  raw: unknown
): ContractInterfaceMetadata | null {
  if (!isRecord(raw) || !Array.isArray(raw.functions)) return null;

  const functions: ContractFunctionSig[] = raw.functions
    .filter(isRecord)
    .map((fn) => ({
      name: asString(fn.name),
      doc: asString(fn.doc),
      inputs: Array.isArray(fn.inputs)
        ? fn.inputs
            .filter(isRecord)
            .map((param) => ({
              name: asString(param.name),
              type_name: asString(param.type_name, 'unknown'),
            }))
            // Drop unnamed params — render code uses `name` in keys.
            .filter((param) => param.name !== '')
        : [],
      outputs: Array.isArray(fn.outputs)
        ? fn.outputs.filter((o): o is string => typeof o === 'string')
        : [],
    }))
    // Drop unnamed functions — render code keys accordions by `name`.
    .filter((fn) => fn.name !== '');

  return {
    functions,
    wasm_byte_len:
      typeof raw.wasm_byte_len === 'number' ? raw.wasm_byte_len : 0,
  };
}

/** Return type for display: `outputs` joined, or `void` when empty. */
export function formatReturnType(outputs: string[]): string {
  return outputs.length > 0 ? outputs.join(', ') : 'void';
}
