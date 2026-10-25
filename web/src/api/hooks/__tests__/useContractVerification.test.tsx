import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { useContractVerification } from '../useContractVerification.js';

import type { ReactNode } from 'react';

// The generated options call the API; replace them with controllable promises.
const answers = vi.hoisted(() => new Map<string, Promise<unknown>>());
vi.mock('@rumblefish/api-types', () => ({
  getVerificationOptions: ({ path }: { path: { contract_id: string } }) => ({
    queryKey: ['verification', path.contract_id],
    queryFn: () => answers.get(path.contract_id),
  }),
}));

const A = 'CD37GB5NIHBI5EGUNIJXEHDNVDDG23HJAVSJTAKSMDQQVKJMBSK6E5EW';
const B = 'CCZOTIJCHZLWCOBZMSYNENXTBH6TVTP3LBEAXYVIIL5UCBGARLYM2XMH';

describe('useContractVerification', () => {
  it("never shows the previous contract's answer while the next one loads", async () => {
    answers.set(A, Promise.resolve({ contract_id: A, status: 'verified' }));
    answers.set(B, new Promise(() => undefined)); // never settles: still loading
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { result, rerender } = renderHook(
      ({ id }) => useContractVerification(id, true),
      { wrapper, initialProps: { id: A } }
    );
    await waitFor(() =>
      expect(result.current.data).toEqual({
        contract_id: A,
        status: 'verified',
      })
    );

    rerender({ id: B });
    expect(result.current.data).toBeUndefined();
    expect(result.current.isPending).toBe(true);
  });

  it('asks nothing until enabled', () => {
    const client = new QueryClient();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(() => useContractVerification(A, false), {
      wrapper,
    });
    expect(result.current.fetchStatus).toBe('idle');
  });
});
