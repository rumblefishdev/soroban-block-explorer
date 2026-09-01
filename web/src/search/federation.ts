import { isAccountId } from '@rumblefish/soroban-block-explorer-ui';

/**
 * SEP-2 federated addresses, both directions, resolved in the browser.
 *
 * Forward (`type=name`): `karol*lobstr.co` → `G…`, for the search box.
 * Reverse (`type=id`): `G…` → `karol*lobstr.co`, for the account page.
 *
 * Both hops are public GETs that serve `Access-Control-Allow-Origin: *`
 * (SEP-1 requires it for `stellar.toml`, and federation servers follow), so
 * the API never issues the request — which is the whole reason this is cheap.
 * The domain is attacker-controlled in the ordinary case, so a server-side
 * version would carry an SSRF surface; here the request leaves the user's own
 * browser and our infrastructure is not in the path at all. See task 0443.
 *
 * The trade that buys: the domain learns the viewer's IP and which account
 * they are looking at. `no-referrer` keeps our origin and path out of it, but
 * the IP is inherent to the browser making the call — accepted deliberately,
 * not overlooked.
 */

/**
 * `name*domain.tld`. Exactly one `*`, no whitespace, and a domain with at
 * least one dot and a plausible TLD — a query that merely contains a star
 * must not cost a network round-trip.
 */
const FEDERATED =
  /^[^*\s]+\*((?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,})$/i;

/** Budget for a whole lookup, both hops together. */
const TIMEOUT_MS = 8_000;

/** How long a domain's `stellar.toml` answer is reused. */
const TOML_TTL_MS = 10 * 60_000;

const FEDERATION_SERVER =
  /^[ \t]*FEDERATION_SERVER[ \t]*=[ \t]*["']([^"'\n]+)["']/im;

export type FederationResolve =
  | { kind: 'resolved'; accountId: string }
  | { kind: 'failed'; reason: string };

/** The domain half of a federated address, or `null` when `q` is not one. */
export function federatedDomain(q: string): string | null {
  const m = FEDERATED.exec(q.trim());
  return m ? m[1].toLowerCase() : null;
}

/**
 * The caller's cancellation and our own timeout, whichever fires first.
 * React Query hands a signal down and aborts it when a key is superseded;
 * without honouring it, every keystroke's lookup runs to completion against
 * a third-party host after its result has already been thrown away.
 */
function budget(signal?: AbortSignal): AbortSignal {
  const timeout = AbortSignal.timeout(TIMEOUT_MS);
  if (signal == null) return timeout;
  // `AbortSignal.any` is recent; fall back to the timeout alone rather than
  // breaking the lookup on an older browser.
  return typeof AbortSignal.any === 'function'
    ? AbortSignal.any([signal, timeout])
    : timeout;
}

async function getText(url: string, signal: AbortSignal): Promise<string> {
  const res = await fetch(url, {
    signal,
    redirect: 'follow',
    // Third-party host: it has no business knowing which page asked, and
    // cookies must never ride along.
    referrerPolicy: 'no-referrer',
    credentials: 'omit',
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.text();
}

type ServerLookup = { server: string } | { kind: 'failed'; reason: string };

/**
 * A domain's `stellar.toml` answer, cached per domain.
 *
 * This is the first hop of BOTH directions, and the flows chain: resolving
 * `karol*lobstr.co` in search redirects to the account page, which then asks
 * the same domain the reverse question. Without the cache that is four
 * round-trips where two do, and browsing several accounts on one domain
 * repeats the toml fetch per account.
 *
 * Stores the href, not a `URL` — callers append their own query parameters,
 * and a shared mutable `URL` would leak one lookup's `q` into the next.
 */
const tomlCache = new Map<string, { at: number; result: ServerLookup }>();

async function federationServerFor(
  domain: string,
  signal: AbortSignal
): Promise<ServerLookup> {
  const hit = tomlCache.get(domain);
  if (hit && Date.now() - hit.at < TOML_TTL_MS) return hit.result;

  const result = await lookupServer(domain, signal);
  tomlCache.set(domain, { at: Date.now(), result });
  return result;
}

async function lookupServer(
  domain: string,
  signal: AbortSignal
): Promise<ServerLookup> {
  let toml: string;
  try {
    toml = await getText(`https://${domain}/.well-known/stellar.toml`, signal);
  } catch {
    return {
      kind: 'failed',
      reason: `${domain} did not serve a stellar.toml, so its federated addresses cannot be resolved.`,
    };
  }

  const declared = FEDERATION_SERVER.exec(toml)?.[1];
  if (declared == null) {
    return {
      kind: 'failed',
      reason: `${domain} publishes a stellar.toml but no federation server, so it cannot resolve names.`,
    };
  }

  let server: URL;
  try {
    server = new URL(declared);
  } catch {
    return {
      kind: 'failed',
      reason: `${domain} declares a federation server that is not a valid URL.`,
    };
  }
  // HTTPS only, on both hops. A plaintext federation answer decides which
  // account the user is sent to, so anyone on the path could redirect them.
  if (server.protocol !== 'https:') {
    return {
      kind: 'failed',
      reason: `${domain} declares a non-HTTPS federation server, which is not called.`,
    };
  }
  return { server: server.href };
}

/** The federation server's URL for one query, built fresh per lookup. */
function query(server: string, q: string, type: 'name' | 'id'): string {
  const url = new URL(server);
  url.searchParams.set('q', q);
  url.searchParams.set('type', type);
  return url.toString();
}

/**
 * Resolve a federated address to its account id.
 *
 * `domain` comes from the caller's own `federatedDomain(address)` — the two
 * always travel together and re-deriving it here would only add a branch no
 * caller can reach.
 *
 * Never throws: every failure comes back as `failed` with a reason the UI
 * shows verbatim. An unresolvable address must not degrade into an empty
 * results page — "no results" claims the address does not exist, which is a
 * different and usually false statement.
 */
export async function resolveFederated(
  address: string,
  domain: string,
  signal?: AbortSignal
): Promise<FederationResolve> {
  const q = address.trim();
  const budgeted = budget(signal);

  const found = await federationServerFor(domain, budgeted);
  if ('kind' in found) return found;

  try {
    const body = await getText(query(found.server, q, 'name'), budgeted);
    const accountId = (JSON.parse(body) as { account_id?: unknown }).account_id;
    // The server is run by the domain owner and can answer with anything.
    // Shape-check before this value becomes a route.
    if (typeof accountId === 'string' && isAccountId(accountId)) {
      return { kind: 'resolved', accountId };
    }
    return {
      kind: 'failed',
      reason: `The federation server for ${domain} answered with something that is not a Stellar account address.`,
    };
  } catch {
    return {
      kind: 'failed',
      reason: `The federation server for ${domain} did not resolve ${q}.`,
    };
  }
}

/**
 * Reverse direction (`type=id`): the federated address a domain claims for
 * an account, or `null`.
 *
 * Only called for accounts that set a `home_domain` on-ledger, which is what
 * makes the answer worth showing: the account named the domain, and the
 * domain names the account back. Both sides have to agree, so the returned
 * address must actually live at that domain — `federationServerFor` proves
 * the first half, the suffix check below proves the second. Without it a
 * domain could claim an account into someone else's namespace.
 *
 * Silent on failure, unlike the forward direction. Nobody asked for this
 * value: it is an attribute the page shows when it exists, so its absence is
 * the ordinary case and not a claim about anything. Measured 2026-09-01 over
 * a 100-account sample carrying a home domain: 85 sat on a domain that
 * publishes a federation server, and 33 of those actually resolved.
 */
export async function resolveFederatedName(
  accountId: string,
  homeDomain: string,
  signal?: AbortSignal
): Promise<string | null> {
  const domain = homeDomain.trim().toLowerCase();
  if (domain.length === 0) return null;

  const budgeted = budget(signal);
  const found = await federationServerFor(domain, budgeted);
  if ('kind' in found) return null;

  let address: unknown;
  try {
    const body = await getText(query(found.server, accountId, 'id'), budgeted);
    address = (JSON.parse(body) as { stellar_address?: unknown })
      .stellar_address;
  } catch {
    return null;
  }

  if (typeof address !== 'string') return null;
  return address.toLowerCase().endsWith(`*${domain}`) ? address : null;
}

/** Test seam: drop the per-domain `stellar.toml` cache. */
export function clearFederationCache(): void {
  tomlCache.clear();
}
