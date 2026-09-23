/** The Stellar Prices API portal — its own SPA, served under `/api/` on this
 *  site's CloudFront distribution (task 0519). Link it with a plain anchor:
 *  the explorer's router would render `/api/` as a 404. */
export const PRICES_API_URL = '/api/';

/** The explorer's own privacy policy — an app route (task 0577). The router
 *  mounts the page here and the footer links to it, so the two cannot drift. */
export const PRIVACY_POLICY_URL = '/privacy-policy';
