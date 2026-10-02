/** The Stellar Prices API portal — its own SPA, served under `/pricing-api/`
 *  on this site's CloudFront distribution (task 0519; `/api/` until task
 *  0608, now a 301). Link it with a plain anchor: the explorer's router would
 *  render `/pricing-api/` as a 404. */
export const PRICES_API_URL = '/pricing-api/';

/** The explorer's own privacy policy — an app route (task 0577). The router
 *  mounts the page here and the footer links to it, so the two cannot drift. */
export const PRIVACY_POLICY_URL = '/privacy-policy';
