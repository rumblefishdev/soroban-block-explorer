import { useDebounced } from '@rumblefish/soroban-block-explorer-ui';
import { useQuery } from '@tanstack/react-query';

import { federationPolicy } from '../api/polling.js';

import {
  federatedDomain,
  resolveFederated,
  resolveFederatedName,
} from './federation.js';
import type { FederationResolve } from './federation.js';

/**
 * The SEP-2 forward lookup as a named hook, so fetching stays out of the
 * components that render it — the seam every other query in this app already
 * uses (`web/src/api/hooks/`). It lives here rather than there because it
 * does not talk to our API at all; its policy is in `api/polling.ts` as
 * `federationPolicy`. The reverse hook (account page) ships with PR #440.
 *
 * Passes React Query's `AbortSignal` down. Without it a superseded lookup
 * keeps running against a third-party host after its result is discarded.
 */

/**
 * Longer than `DEFAULT_DEBOUNCE_MS` (300) on purpose — see the note on
 * `useFederatedAddress`.
 */
const FEDERATION_DEBOUNCE_MS = 500;

/**
 * Forward: a federated address the user typed → the account it names.
 *
 * Debounces and classifies here rather than taking both from the caller, and
 * waits longer than the app-wide 300 ms: this is the one query that leaves for
 * a host the typed text chooses, so a pause mid-word must not be enough to
 * dial it.
 * Typing `bob*lobstr.com` passes through `bob*lobstr.co` — a real domain, a
 * valid federated shape, and not the one the user meant; undebounced it
 * receives a request and with it the viewer's IP. Owning the delay means no
 * caller can drop it. This is a deliberate second timer next to the one in
 * `useSearchResults` — a /simplify pass tried removing it and the debounce
 * test failed at once, because the first caller to break the "already
 * debounced" contract was the test harness itself.
 */
export function useFederatedAddress(address: string): {
  /** The domain being resolved, or `null` when the input is not federated. */
  domain: string | null;
  data: FederationResolve | undefined;
} {
  const settled = useDebounced(address, FEDERATION_DEBOUNCE_MS).trim();
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
 * claims for it, `null` when there is none, `undefined` while it is being
 * asked. No debounce here — the input is an account id from a loaded page,
 * not something being typed.
 */
export function useFederatedName(
  accountId: string,
  homeDomain: string
): string | null | undefined {
  return useQuery({
    queryKey: ['federatedName', accountId, homeDomain],
    queryFn: ({ signal }) =>
      resolveFederatedName(accountId, homeDomain, signal),
    enabled: homeDomain.length > 0,
    ...federationPolicy,
  }).data;
}
