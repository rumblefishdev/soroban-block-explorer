import { forwardRef } from 'react';
import { Link, type LinkProps } from 'react-router-dom';

export type RouterLinkAdapterProps = Omit<LinkProps, 'to'> & { href?: string };

/**
 * Adapts `libs/ui`'s `href`-based link API to react-router's `to`, so internal
 * identifier links (`IdentifierDisplay` and everything built on it) navigate
 * client-side instead of triggering a full page reload. Installed once via
 * `LinkComponentProvider` at the router root. See lore-0384.
 *
 * Renders a real `<a href>` (react-router `Link`), so copy-link,
 * middle-click and cmd/ctrl-click "open in new tab" keep working.
 */
export const RouterLinkAdapter = forwardRef<
  HTMLAnchorElement,
  RouterLinkAdapterProps
>(function RouterLinkAdapter({ href, ...props }, ref) {
  return <Link ref={ref} to={href ?? ''} {...props} />;
});
