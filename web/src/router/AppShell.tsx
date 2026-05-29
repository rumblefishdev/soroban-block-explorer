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
}: {
  height: number;
  onClick: (e: React.MouseEvent<HTMLAnchorElement>) => void;

  variant?: 'soroban' | 'rumblefish';
}) {
  const isSoroban = variant === 'soroban';
  return (
    <Box
      component="a"
      href={routes.home}
      aria-label={isSoroban ? 'Soroban Explorer — home' : 'Rumblefish — home'}
      onClick={onClick}
      sx={{ display: 'inline-flex', lineHeight: 0 }}
    >
      <Box
        component="img"
        src={isSoroban ? '/soroban-logo.webp' : '/rumblefish-logo.svg'}
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

  const FOOTER_NAV_ITEMS = FOOTER_EXPLORER_LINKS.map((item) => ({
    label: item.label,
    href: item.href,
    onClick: (e: React.MouseEvent<HTMLAnchorElement>) =>
      handleFooterNavClick(item.href, e),
  }));

  return (
    <Box sx={{ minHeight: '100vh', display: 'flex', flexDirection: 'column' }}>
      {!isHome && (
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
      )}
      <SecondaryNav
        logo={<HomeLogo height={24} onClick={handleHomeClick} />}
        navItems={NAV_ITEMS}
        activePage={activePage}
        onNavClick={handleNavClick}
      />
      <Box sx={{ flex: 1, position: 'relative', width: '100%' }}>
        <PageGridBackdrop />
        {isHome && <HomeHeroGlow />}
        <Box
          component="main"
          sx={{
            width: '100%',
            maxWidth: grid.desktop.maxWidth,
            mx: 'auto',

            px: {
              xs: `${grid.mobile.margin}px`,
              md: `${grid.desktop.margin}px`,
            },
            py: { xs: 2, md: 4 },
            position: 'relative',
            zIndex: 1,
          }}
        >
          <Outlet />
        </Box>
      </Box>
      <Footer
        logo={
          <HomeLogo
            height={47}
            onClick={handleHomeClick}
            variant="rumblefish"
          />
        }
        navItems={FOOTER_NAV_ITEMS}
      />
    </Box>
  );
}
