// Which Stellar network this build of the explorer shows (task 0553). The same
// SPA is deployed twice, once per network, and each build is told which one it
// is through VITE_STELLAR_NETWORK.

export type Network = 'mainnet' | 'testnet';

export interface NetworkSite {
  key: Network;
  name: string;
  about: string;
  url: string;
}

// Both deployments, in the order the network menu lists them. The hosts are
// the `domainName` of infra/envs/production.json and testnet.json.
export const NETWORK_SITES: NetworkSite[] = [
  {
    key: 'mainnet',
    name: 'Mainnet',
    about: 'The live Stellar network. Real assets.',
    url: 'https://sorobanscan.rumblefish.dev',
  },
  {
    key: 'testnet',
    name: 'Testnet',
    about: 'Test data for developers. No value; reset a few times a year.',
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

/** On testnet, the browser tab says so too: "Testnet · …" in the title and a
 *  yellow dot on the icon, so a testnet tab is told apart among others. Every
 *  icon file has a testnet twin (`favicon.svg` → `favicon-testnet.svg`,
 *  `favicon-32.png` → `favicon-testnet-32.png`, …): browsers that skip the SVG
 *  icon, Safari among them, take a PNG. */
export function markTestnetTab(current: Network) {
  if (current !== 'testnet') return;
  document.title = `Testnet · ${document.title}`;
  for (const icon of document.querySelectorAll('link[rel="icon"]')) {
    const href = icon.getAttribute('href') ?? '';
    icon.setAttribute('href', href.replace('/favicon', '/favicon-testnet'));
  }
}
