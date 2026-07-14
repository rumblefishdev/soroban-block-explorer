import { RouterProvider } from 'react-router-dom';

import { LinkComponentProvider } from '@rumblefish/soroban-block-explorer-ui';

import { router } from './router/index.js';
import { RouterLinkAdapter } from './router/RouterLinkAdapter.js';

// Route internal identifier links (IdentifierDisplay et al.) through
// react-router so list→detail navigation is client-side, not a full page
// reload (lore-0384). Placed above RouterProvider so it also covers
// errorElement routes — React context still flows into the route tree that
// RouterProvider renders, and RouterLinkAdapter only renders its Link inside
// a route (where router context exists).
export function App() {
  return (
    <LinkComponentProvider value={RouterLinkAdapter}>
      <RouterProvider router={router} />
    </LinkComponentProvider>
  );
}
