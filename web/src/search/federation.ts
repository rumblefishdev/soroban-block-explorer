import { isAccountId } from '@rumblefish/soroban-block-explorer-ui';

/**
 * SEP-2 federated address resolution, forward direction (`type=name`):
 * `karol*lobstr.co` → `G…`.
 *
 * Runs entirely in the browser. Both hops are public GETs that serve
 * `Access-Control-Allow-Origin: *` (SEP-1 requires it for `stellar.toml`,
 * and federation servers follow), so the API never issues the request —
 * which is the whole reason this direction is cheap. The domain is
 * attacker-controlled in the ordinary case (the user types it), so the
 * server-side version of this would carry an SSRF surface; here the
 * request leaves the user's own browser, to a host the user named, and
 * our infrastructure is not in the path at all. See task 0443 scope A.
 *
 * The reverse direction (`G…` → `name*domain`, scope B) lives here too, in
 * `resolveFederatedName` — measurement showed it can run in the browser for
 * the same reason, so it never grew the backend the task first sketched.
 */

/**
 * `name*domain.tld`. Exactly one `*`, no whitespace, and a domain with at
 * least one dot and a plausible TLD — a query that merely contains a star
 * must not cost a network round-trip.
 */
const FEDERATED =
  /^[^*\s]+\*((?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,})$/i;

const TIMEOUT_MS = 8_000;

/** ponytail: post-hoc length check, not a streaming cap — the timeout is
 *  what actually bounds a hostile server. Enough to stop a stray large
 *  file from being parsed; swap for a reader if a real abuse case shows up. */
const MAX_CHARS = 100_000;

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

async function getBounded(url: string, signal: AbortSignal): Promise<string> {
  const res = await fetch(url, { signal, redirect: 'follow' });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const text = await res.text();
  if (text.length > MAX_CHARS) throw new Error('response too large');
  return text;
}

/**
 * The federation server a domain publishes, or the reason there is none.
 * The first hop of any federation lookup, whichever way it then queries.
 */
async function federationServerFor(
  domain: string,
  signal: AbortSignal
): Promise<{ server: URL } | { reason: string }> {
  let toml: string;
  try {
    toml = await getBounded(
      `https://${domain}/.well-known/stellar.toml`,
      signal
    );
  } catch {
    return {
      reason: `${domain} did not serve a stellar.toml, so its federated addresses cannot be resolved.`,
    };
  }

  const declared = FEDERATION_SERVER.exec(toml)?.[1];
  if (declared == null) {
    return {
      reason: `${domain} publishes a stellar.toml but no federation server, so it cannot resolve names.`,
    };
  }

  let server: URL;
  try {
    server = new URL(declared);
  } catch {
    return {
      reason: `${domain} declares a federation server that is not a valid URL.`,
    };
  }
  // HTTPS only, on both hops. A plaintext federation answer decides which
  // account the user is sent to, so anyone on the path could redirect them.
  if (server.protocol !== 'https:') {
    return {
      reason: `${domain} declares a non-HTTPS federation server, which is not called.`,
    };
  }
  return { server };
}

/**
 * Resolve a federated address to its account id.
 *
 * Never throws: every failure mode comes back as `failed` with a reason the
 * UI shows verbatim. An unresolvable address must not degrade into an empty
 * results page — "no results" is the claim that the address does not exist,
 * which is a different and usually false statement.
 */
export async function resolveFederated(
  address: string
): Promise<FederationResolve> {
  const q = address.trim();
  const domain = federatedDomain(q);
  if (domain == null) {
    return { kind: 'failed', reason: `${q} is not a federated address.` };
  }

  const signal = AbortSignal.timeout(TIMEOUT_MS);

  const found = await federationServerFor(domain, signal);
  if ('reason' in found) return { kind: 'failed', reason: found.reason };
  const { server } = found;

  server.searchParams.set('q', q);
  server.searchParams.set('type', 'name');

  let body: string;
  try {
    body = await getBounded(server.toString(), signal);
  } catch {
    return {
      kind: 'failed',
      reason: `The federation server for ${domain} did not resolve ${q}.`,
    };
  }

  let accountId: unknown;
  try {
    accountId = (JSON.parse(body) as { account_id?: unknown }).account_id;
  } catch {
    accountId = undefined;
  }
  // The server is run by the domain owner and can answer with anything.
  // Shape-check before this value becomes a route.
  if (typeof accountId !== 'string' || !isAccountId(accountId)) {
    return {
      kind: 'failed',
      reason: `The federation server for ${domain} answered with something that is not a Stellar account address.`,
    };
  }

  return { kind: 'resolved', accountId };
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
  homeDomain: string
): Promise<string | null> {
  const domain = homeDomain.trim().toLowerCase();
  if (domain.length === 0 || !isAccountId(accountId)) return null;

  const signal = AbortSignal.timeout(TIMEOUT_MS);
  const found = await federationServerFor(domain, signal);
  if ('reason' in found) return null;

  const { server } = found;
  server.searchParams.set('q', accountId);
  server.searchParams.set('type', 'id');

  let address: unknown;
  try {
    const body = await getBounded(server.toString(), signal);
    address = (JSON.parse(body) as { stellar_address?: unknown })
      .stellar_address;
  } catch {
    return null;
  }

  if (typeof address !== 'string') return null;
  return address.toLowerCase().endsWith(`*${domain}`) ? address : null;
}
