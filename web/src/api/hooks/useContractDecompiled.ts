import { getDecompiledOptions } from '@rumblefish/api-types';
import { useQuery } from '@tanstack/react-query';

import { detailPolicy } from '../polling.js';

/**
 * Fetches the contract's decompiled source
 * (`GET /contracts/:contract_id/decompiled`, task 0465). `format=rust`
 * (default) may degrade to WAT in the same response (`representation:
 * "wat"` + `rust_error`) when Rust emission fails. The output is immutable
 * per (wasm_hash, decompiler version), so a fetched representation never
 * goes stale. Disabled until an id is present.
 */
export const useContractDecompiled = (
  contractId: string,
  format: 'rust' | 'wat'
) =>
  useQuery({
    ...getDecompiledOptions({
      path: { contract_id: contractId },
      query: { format },
    }),
    ...detailPolicy,
    staleTime: Infinity,
    enabled: contractId.length > 0,
  });
