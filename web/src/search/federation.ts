import { isAccountId } from '@rumblefish/soroban-block-explorer-ui';

/**
 * SEP-2 federated addresses, resolved in the browser.
 *
 * Forward (`type=name`): `karol*lobstr.co` → `G…`, for the search box.
 * The reverse direction (`type=id`, for the account page) ships separately
 * with PR #440 on feat/0443_sep2-federated-addresses.
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
  return signal == null ? timeout : AbortSignal.any([signal, timeout]);
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

// No cache below this line on purpose: React Query already caches the whole
// resolve per address (`federationPolicy.staleTime`), and a module-level
// per-domain cache proved worse than none — an aborted first hop would have
// been stored as "no stellar.toml" and served for its TTL.
type ServerLookup = { server: string } | { kind: 'failed'; reason: string };

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

/** The federation server's URL for one forward query, built fresh per lookup. */
function query(server: string, q: string): string {
  const url = new URL(server);
  url.searchParams.set('q', q);
  url.searchParams.set('type', 'name');
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

  const found = await lookupServer(domain, budgeted);
  if ('kind' in found) return found;

  try {
    const body = await getText(query(found.server, q), budgeted);
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
