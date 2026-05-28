import { useRef, useState } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';
import Box from '@mui/material/Box';

import {
  TopNav,
  SecondaryNav,
  Footer,
  PageGridBackdrop,
  grid,
  type NavItem,
} from '@rumblefish/soroban-block-explorer-ui';

import { useNetworkStats } from '../api/index.js';
import { directRouteFor } from '../search/directRouteFor.js';
import { GlobalSearchBar } from '../search/GlobalSearchBar.js';
import { NAV_LINKS, routes } from './routes.js';

const NAV_ITEMS: NavItem[] = NAV_LINKS.map((link) => ({
  label: link.label,
  href: link.to,
}));

function isModifiedClick(e: React.MouseEvent): boolean {
  return e.metaKey || e.ctrlKey || e.shiftKey || e.altKey;
}

function HomeLogo({
  height,
  onClick,
}: {
  height: number;
  onClick: (e: React.MouseEvent<HTMLAnchorElement>) => void;
}) {
  return (
    <Box
      component="a"
      href={routes.home}
      aria-label="Stellar Explorer — home"
      onClick={onClick}
      sx={{ display: 'inline-flex', lineHeight: 0 }}
    >
      <img
        src="/rumblefish-logo.svg"
        alt="Rumblefish"
        style={{ height, display: 'block' }}
      />
    </Box>
  );
}

function useActivePage(): string | undefined {
  const { pathname } = useLocation();
  const match = NAV_LINKS.find((link) =>
    link.to === routes.home
      ? pathname === routes.home
      : pathname.startsWith(link.to)
  );
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

  // The home page is full-bleed (hero band, edge-to-edge section
  // backgrounds); every other route gets the standard content padding.
  const isFullBleed = pathname === routes.home;

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

  const FOOTER_NAV_ITEMS = NAV_ITEMS.map((item) => {
    const href = item.href;
    return {
      ...item,
      onClick: href
        ? (e: React.MouseEvent<HTMLAnchorElement>) =>
            handleFooterNavClick(href, e)
        : undefined,
    };
  });

  return (
    <Box sx={{ minHeight: '100vh', display: 'flex', flexDirection: 'column' }}>
      <TopNav
        stats={stats}
        searchValue={searchValue}
        onSearchChange={handleSearchChange}
        onSearchSubmit={handleSearchSubmit}
        onSearchClear={handleSearchClear}
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
      <SecondaryNav
        logo={<HomeLogo height={32} onClick={handleHomeClick} />}
        navItems={NAV_ITEMS}
        activePage={activePage}
        onNavClick={handleNavClick}
      />
      <Box
        component="main"
        sx={{ position: 'relative', flex: 1, width: '100%' }}
      >
        {/* Faint grid halo behind every page, full-bleed so it reads the
            same on every route. The home page adds the warm gold glow
            pills on top of this same backdrop; every other route shows
            the grid on its own. Lives on the full-width <main> (not the
            constrained content box) so the grid spans the viewport
            instead of being clipped to the centered column. */}
        <PageGridBackdrop />
        <Box
          sx={{
            position: 'relative',
            zIndex: 1,
            width: '100%',
            maxWidth: isFullBleed ? 'none' : grid.desktop.maxWidth,
            mx: 'auto',
            ...(isFullBleed ? {} : { px: `${grid.desktop.margin}px`, py: 4 }),
          }}
        >
          <Outlet />
        </Box>
      </Box>
      <Footer
        logo={<HomeLogo height={47} onClick={handleHomeClick} />}
        navItems={FOOTER_NAV_ITEMS}
      />
    </Box>
  );
}
