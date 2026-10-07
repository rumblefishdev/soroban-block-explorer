// Which Stellar network this build of the explorer shows (task 0553). The same
// SPA is deployed twice, once per network, and each build is told which one it
// is through VITE_STELLAR_NETWORK.

import { PRICES_API_URL } from '@rumblefish/soroban-block-explorer-ui';

export type Network = 'mainnet' | 'testnet';

export interface NetworkSite {
  key: Network;
  name: string;
  url: string;
}

const MAINNET_SITE_URL = 'https://sorobanscan.rumblefish.dev';

// Both deployments, in the order the network menu lists them. The hosts are
// the `domainName` of infra/envs/production.json and testnet.json.
export const NETWORK_SITES: NetworkSite[] = [
  {
    key: 'mainnet',
    name: 'Mainnet',
    url: MAINNET_SITE_URL,
  },
  {
    key: 'testnet',
    name: 'Testnet',
    url: 'https://testnet.sorobanscan.rumblefish.dev',
  },
];

/** Unset means mainnet: the production build has always been built without
 *  the variable. Anything other than the two names is a typo in a build
 *  recipe, and a testnet page that looks like mainnet is the one mistake this
 *  marker exists to prevent, so it stops the page instead of guessing. */
export function networkFrom(raw: string | undefined): Network {
  if (!raw || raw === 'mainnet') return 'mainnet';
  if (raw === 'testnet') return 'testnet';
  throw new Error(
    `VITE_STELLAR_NETWORK is "${raw}"; expected "mainnet" or "testnet".`
  );
}

export const network: Network = networkFrom(
  import.meta.env.VITE_STELLAR_NETWORK
);

/** The Prices API portal ships with the mainnet site only, and its prices are
 *  mainnet prices (task 0519). Mainnet links it on its own host; testnet
 *  links to mainnet's, since its own host would answer with the explorer's
 *  404 page. */
export function pricesApiUrlFor(current: Network): string {
  if (current === 'mainnet') return PRICES_API_URL;
  return `${MAINNET_SITE_URL}${PRICES_API_URL}`;
}

// Each icon index.html declares, and its testnet twin with a yellow dot. All
// of them: Safari skips the SVG for a PNG, and iOS uses the touch icon for a
// home-screen shortcut.
const TESTNET_ICONS: Record<string, string> = {
  '/favicon.svg': '/favicon-testnet.svg',
  '/favicon-32.png': '/favicon-testnet-32.png',
  '/favicon-16.png': '/favicon-testnet-16.png',
  '/apple-touch-icon.png': '/apple-touch-icon-testnet.png',
};

/** On testnet, the browser tab says so too: "Testnet · …" in the title and a
 *  yellow dot on the icon, so a testnet tab is told apart among others. */
export function markTestnetTab(current: Network) {
  if (current !== 'testnet') return;
  document.title = `Testnet · ${document.title}`;
  for (const link of document.querySelectorAll('link[href]')) {
    const testnetIcon = TESTNET_ICONS[link.getAttribute('href') ?? ''];
    if (testnetIcon) link.setAttribute('href', testnetIcon);
  }
}
