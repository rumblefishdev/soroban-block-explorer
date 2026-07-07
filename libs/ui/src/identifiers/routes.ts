import type { EntityType } from './types.js';

/**
 * Canonical URL path SEGMENTS — the single source of truth for the plural
 * segment word of every entity route (`transactions`, `liquidity-pools`, …).
 *
 * Consumed by `getIdentifierHref` below (the URL builder), by
 * `web/src/router/routes.ts` (index routes + `NAV_LINKS`) and by
 * `web/src/router/index.tsx` (the React Router `path` patterns). Renaming a
 * segment (e.g. `/liquidity-pools` → `/pools`) is therefore a one-line edit
 * here — the route builders and the router matcher stay in lock-step (task
 * 0299 acceptance criteria).
 */
export const routeSegments = {
  transaction: 'transactions',
  ledger: 'ledgers',
  account: 'accounts',
  asset: 'assets',
  contract: 'contracts',
  nft: 'nfts',
  pool: 'liquidity-pools',
} as const satisfies Record<EntityType, string>;

/**
 * The single URL builder: a detail-page href from an entity type + id
 * (`/accounts/:id`, `/assets/:id`, …), assembled from {@link routeSegments}.
 *
 * The one place that turns `(type, id)` into a URL. `IdentifierDisplay` uses
 * it for static links; `web`'s `routes` and search `routeForHit` re-use it
 * (the `web → libs/ui` dependency direction is allowed; the reverse is not,
 * which is why the source lives here).
 *
 * NFT is unsupported: its identity is composite `(contract_id, token_id)`
 * per ADR 0030 / task 0264 Phase 8a, so a single-id build cannot produce a
 * valid `/nfts/:c/:t` URL. NFT routing is built inline from a full hit
 * (`web`'s `routeForHit` and `routes.nft`). Loud throw beats a silent broken
 * URL if a future regression routes an `'nft'` here.
 */
export function getIdentifierHref(type: EntityType, id: string): string {
  if (type === 'nft') {
    throw new Error(
      'getIdentifierHref("nft", id) is not supported — NFT routing is ' +
        'composite `(contract_id, token_id)`. Build the `/nfts/:c/:t` URL ' +
        'from the hit, or pass `href` explicitly to `IdentifierDisplay`.'
    );
  }
  return `/${routeSegments[type]}/${encodeURIComponent(id)}`;
}
