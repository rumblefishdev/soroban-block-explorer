import { useEffect, useRef, useState } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';
import Box from '@mui/material/Box';
import { useTheme } from '@mui/material/styles';

import {
  TopNav,
  SecondaryNav,
  Footer,
  PageGridBackdrop,
  grid,
  type NavItem,
} from '@rumblefish/soroban-block-explorer-ui';

import { useNetworkStats } from '../api/index.js';
import { HomeHeroGlow } from '../pages/home/HomeHeroGlow.js';
import { directRouteFor } from '../search/directRouteFor.js';
import { GlobalSearchBar } from '../search/GlobalSearchBar.js';
import { NAV_LINKS, routes } from './routes.js';

const NAV_ITEMS: NavItem[] = NAV_LINKS.map((link) => ({
  label: link.label,
  href: link.to,
}));

const FOOTER_EXPLORER_LINKS: { label: string; href: string }[] = [
  { label: 'Home', href: routes.home },
  { label: 'Search', href: routes.search('') },
  { label: 'Transactions', href: routes.transactions },
  { label: 'Ledgers', href: routes.ledgers },
  { label: 'Liquidity Pools', href: routes.pools },
  { label: 'Tokens', href: routes.assets },
  { label: 'Contracts', href: routes.contracts },
  { label: 'Accounts', href: routes.accounts },
  { label: 'NFTs', href: routes.nfts },
];

function isModifiedClick(e: React.MouseEvent): boolean {
  return e.metaKey || e.ctrlKey || e.shiftKey || e.altKey;
}

function HomeLogo({
  height,
  onClick,
  variant = 'soroban',
  href = routes.home,
  external = false,
}: {
  height: number;
  onClick?: (e: React.MouseEvent<HTMLAnchorElement>) => void;

  variant?: 'soroban' | 'rumblefish';
  /** Link target. Defaults to the explorer home. */
  href?: string;
  /** Open `href` in a new tab (for off-site links like rumblefish.dev). */
  external?: boolean;
}) {
  const isSoroban = variant === 'soroban';
  // Both lockups are raster artwork drawn for ONE background: the default
  // files carry light glyphs for the dark surface, the `-light` files are the
  // same artwork remapped to ink for the light surface. Picking by brand
  // variant alone left the nav "Scan" wordmark and the whole footer lockup
  // white-on-white.
  const base = isSoroban ? '/soroban-logo' : '/rumblefish-logo';
  const suffix = useTheme().palette.mode === 'light' ? '-light' : '';
  return (
    <Box
      component="a"
      href={href}
      {...(external && { target: '_blank', rel: 'noopener noreferrer' })}
      aria-label={
        isSoroban ? 'Soroban Explorer — home' : 'Rumblefish — rumblefish.dev'
      }
      onClick={onClick}
      sx={{ display: 'inline-flex', lineHeight: 0 }}
    >
      <Box
        component="img"
        src={`${base}${suffix}.webp`}
        alt={isSoroban ? 'Soroban' : 'Rumblefish'}
        sx={{ height, width: 'auto', display: 'block' }}
      />
    </Box>
  );
}

function useActivePage(): string | undefined {
  const { pathname } = useLocation();
  const match = NAV_LINKS.find((link) => pathname.startsWith(link.to));
  return match?.label;
}

export function AppShell() {
  const navigate = useNavigate();
  const activePage = useActivePage();
  const { pathname } = useLocation();
  // Live network counters for TopNav. `undefined` while loading or
  // errored — TopNav renders dashes so we don't ship visually-
  // misleading hard-coded zeros.
  const { data: stats } = useNetworkStats();
  const [searchValue, setSearchValue] = useState('');
  const [searchOpen, setSearchOpen] = useState(false);

  const enterHandlerRef = useRef<() => boolean>(() => false);

  const isHome = pathname === routes.home;

  // Scroll back to the top whenever the PATH changes (navigating to another
  // page / detail). Keyed on `pathname` only — table pagination and filters
  // change just the `?search` (cursor / type) params, NOT the path, so those
  // keep the current scroll position (next/prev stays put, as before).
  useEffect(() => {
    window.scrollTo(0, 0);
  }, [pathname]);

  // Global Cmd/Ctrl+K focuses the visible search input (header on most routes,
  // hero on home) — the shortcut the "CTRL + K" pill advertises (F-RR-2).
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        const el = document.querySelector<HTMLElement>('[data-global-search]');
        if (el) {
          e.preventDefault();
          el.focus();
        }
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const handleSearchSubmit = () => {
    if (enterHandlerRef.current()) return;
    const q = searchValue.trim();
    if (q) {
      setSearchOpen(false);
      void navigate(directRouteFor(q) ?? routes.search(q));
    }
  };

  const handleSearchChange = (value: string) => {
    setSearchValue(value);
    if (value.length > 0) setSearchOpen(true);
  };

  const handleSearchClear = () => {
    setSearchValue('');
    setSearchOpen(false);
  };

  // Re-focusing the search field while it already holds a query should bring
  // the results dropdown back (it stays dismissed after a click-away even
  // though the text remains).
  const handleSearchFocus = () => {
    if (searchValue.trim().length > 0) setSearchOpen(true);
  };

  const showSearchOverlay = searchOpen && searchValue.trim().length > 0;

  const handleNavClick = (item: NavItem) => {
    if (item.href) void navigate(item.href);
  };

  const handleHomeClick = (e: React.MouseEvent<HTMLAnchorElement>) => {
    if (isModifiedClick(e)) return;
    e.preventDefault();
    void navigate(routes.home);
  };

  const handleFooterNavClick = (
    href: string,
    e: React.MouseEvent<HTMLAnchorElement>
  ) => {
    if (isModifiedClick(e)) return;
    e.preventDefault();
    void navigate(href);
  };

  const FOOTER_NAV_ITEMS = FOOTER_EXPLORER_LINKS.map((item) => ({
    label: item.label,
    href: item.href,
    onClick: (e: React.MouseEvent<HTMLAnchorElement>) =>
      handleFooterNavClick(item.href, e),
  }));

  return (
    <Box sx={{ minHeight: '100vh', display: 'flex', flexDirection: 'column' }}>
      {/* Both nav bars stick to the top while the page scrolls. `zIndex` above
          the page content; each bar keeps its own internal z-order (TopNav's
          search dropdown over SecondaryNav). */}
      <Box
        sx={(theme) => ({
          position: 'sticky',
          top: 0,
          zIndex: theme.zIndex.topNav,
        })}
      >
        {!isHome && (
          <TopNav
            stats={stats}
            searchValue={searchValue}
            onSearchChange={handleSearchChange}
            onSearchSubmit={handleSearchSubmit}
            onSearchClear={handleSearchClear}
            onSearchFocus={handleSearchFocus}
            searchOverlaySlot={
              showSearchOverlay ? (
                <GlobalSearchBar
                  q={searchValue}
                  onDismiss={() => setSearchOpen(false)}
                  registerEnterHandler={(handler) => {
                    enterHandlerRef.current = handler;
                  }}
                />
              ) : undefined
            }
          />
        )}
        <SecondaryNav
          logo={<HomeLogo height={24} onClick={handleHomeClick} />}
          navItems={NAV_ITEMS}
          activePage={activePage}
          onNavClick={handleNavClick}
        />
      </Box>
      <Box sx={{ flex: 1, position: 'relative', width: '100%' }}>
        <PageGridBackdrop />
        {isHome && <HomeHeroGlow />}
        <Box
          component="main"
          sx={(theme) => ({
            width: '100%',
            maxWidth: grid.desktop.maxWidth,
            mx: 'auto',

            px: {
              xs: `${grid.mobile.margin}px`,
              md: `${grid.desktop.margin}px`,
            },
            py: { xs: 2, md: 4 },
            position: 'relative',
            zIndex: theme.zIndex.contentMain,
          })}
        >
          <Outlet />
        </Box>
      </Box>
      <Footer
        logo={
          <HomeLogo
            height={47}
            variant="rumblefish"
            href="https://www.rumblefish.dev"
            external
          />
        }
        navItems={FOOTER_NAV_ITEMS}
      />
    </Box>
  );
}
