import {
  DEFAULT_DEBOUNCE_MS,
  useDebounced,
} from '@rumblefish/soroban-block-explorer-ui';
import { useQuery } from '@tanstack/react-query';

import { federationPolicy } from '../api/polling.js';

import {
  federatedDomain,
  resolveFederated,
  resolveFederatedName,
} from './federation.js';
import type { FederationResolve } from './federation.js';

/**
 * The two SEP-2 lookups as named hooks, so fetching stays out of the
 * components that render it — the seam every other query in this app already
 * uses (`web/src/api/hooks/`). They live here rather than there because they
 * do not talk to our API at all; the policy they share is in
 * `api/polling.ts` as `federationPolicy`.
 *
 * Both pass React Query's `AbortSignal` down. Without it a superseded lookup
 * keeps running against a third-party host after its result is discarded.
 */

/**
 * Forward: a federated address the user typed → the account it names.
 *
 * Debounces and classifies here rather than taking both from the caller.
 * Typing `bob*lobstr.com` passes through `bob*lobstr.co` — a real domain, a
 * valid federated shape, and not the one the user meant; undebounced it
 * receives a request and with it the viewer's IP. Owning the delay means no
 * caller can drop it, and it survives a test that stubs the search hook.
 */
export function useFederatedAddress(address: string): {
  /** The domain being resolved, or `null` when the input is not federated. */
  domain: string | null;
  data: FederationResolve | undefined;
} {
  const settled = useDebounced(address, DEFAULT_DEBOUNCE_MS).trim();
  const domain = federatedDomain(settled);
  const query = useQuery({
    queryKey: ['federatedAddress', settled],
    queryFn: ({ signal }) => resolveFederated(settled, domain ?? '', signal),
    enabled: domain != null,
    ...federationPolicy,
  });
  return { domain, data: query.data };
}

/**
 * Reverse: an account and its on-ledger home domain → the address that domain
 * claims for it, or `null` when there is none.
 */
export function useFederatedName(
  accountId: string,
  homeDomain: string
): { data: string | null | undefined; isPending: boolean } {
  const query = useQuery({
    queryKey: ['federatedName', accountId, homeDomain],
    queryFn: ({ signal }) =>
      resolveFederatedName(accountId, homeDomain, signal),
    enabled: homeDomain.length > 0,
    ...federationPolicy,
  });
  return {
    data: query.data,
    isPending: homeDomain.length > 0 && query.data === undefined,
  };
}
