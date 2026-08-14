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
    // The global policy retries once on 5xx, which is right for a flaky
    // backend but wrong here: `decompile_failed` is deterministic for a
    // given (wasm_hash, decompiler version), and each attempt burns the
    // endpoint's full 10 s timeout. Failing fast lets the UI fall back to
    // WAT in ~10 s instead of ~20 s. Other 5xx keep the global behaviour.
    retry: (failureCount, error) => {
      const code = (error as { body?: { code?: unknown } })?.body?.code;
      if (code === 'decompile_failed') return false;
      const status = (error as { status?: number })?.status;
      if (typeof status === 'number' && status >= 400 && status < 500) {
        return false;
      }
      return failureCount < 1;
    },
  });
