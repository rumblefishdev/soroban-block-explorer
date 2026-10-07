/** The Stellar Prices API portal — its own SPA, served under `/prices-api/`
 *  on the mainnet site's CloudFront distribution only (task 0519; `/api/`
 *  until task 0608, now a 301). The testnet site has no portal, so the app
 *  decides the full link per network and hands it to the layout. Link it with
 *  a plain anchor: the explorer's router would render `/prices-api/` as a
 *  404. */
export const PRICES_API_URL = '/prices-api/';

/** The explorer's own privacy policy — an app route (task 0577). The router
 *  mounts the page here and the footer links to it, so the two cannot drift. */
export const PRIVACY_POLICY_URL = '/privacy-policy';
