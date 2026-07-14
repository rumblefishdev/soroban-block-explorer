import { createContext, useContext, type ElementType } from 'react';

/**
 * The component used to render **internal** entity links (identifiers → route
 * paths). Defaults to a native `<a>` so `libs/ui` stays router-agnostic — but a
 * bare `<a>` click is a full page reload, not SPA navigation. The app installs
 * an adapter around its router's link (e.g. react-router's `Link`) via
 * {@link LinkComponentProvider} so internal navigation stays client-side (no
 * reload → no remount of the shell / TopNav stats bar; see lore-0384).
 *
 * The provided component MUST accept the same `href` prop as `<a>` (the app's
 * adapter maps `href` → its router's `to`).
 */
const LinkComponentContext = createContext<ElementType>('a');

export const LinkComponentProvider = LinkComponentContext.Provider;

export const useLinkComponent = (): ElementType =>
  useContext(LinkComponentContext);
