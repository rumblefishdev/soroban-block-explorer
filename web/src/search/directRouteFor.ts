import { routes } from '../router/routes.js';

/**
 * Client-side direct-redirect classifier for inputs that the backend
 * search redirect path doesn't cover.
 *
 * Today the only case is a bare-digit ledger sequence — backend
 * `/v1/search` has no ledger redirect branch (ledger is not a search
 * bucket entity), so the FE handles it before calling the API.
 *
 * Backend-served direct redirects (tx hash, full G/C/L strkey) flow
 * through `/v1/search` → `SearchResponse::Redirect` → `routeForHit`.
 * Adding more FE shortcuts here is rarely the right call — keep
 * classifier logic on the server unless the entity type has no
 * search bucket.
 *
 * Returns the target route when the input matches a known
 * deterministic shape, or `null` to fall through to the results page.
 */
export function directRouteFor(q: string): string | null {
  const trimmed = q.trim();
  // Bare-digit u32 → ledger sequence redirect.
  //
  // Stellar ledger sequences are `u32`, currently in the low millions
  // on mainnet. Cap at `0xFFFFFFFF` to reject inputs that parse to a
  // number but would overflow the path-validator on the backend
  // (`common::path::sequence`). Reject `0` because sequence numbering
  // starts at 1 (the genesis ledger is sequence 1).
  if (/^\d+$/.test(trimmed)) {
    const n = Number(trimmed);
    if (Number.isInteger(n) && n > 0 && n <= 0xffffffff) {
      return routes.ledger(n);
    }
  }
  return null;
}
