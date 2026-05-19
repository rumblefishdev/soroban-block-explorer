import { useRef, useState } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';
import Box from '@mui/material/Box';

import {
  TopNav,
  SecondaryNav,
  Footer,
  type Network,
  type NavItem,
} from '@rumblefish/soroban-block-explorer-ui';

import { GlobalSearchBar } from '../search/GlobalSearchBar.js';
import { NAV_LINKS, routes } from './routes.js';

const NAV_ITEMS: NavItem[] = NAV_LINKS.map((link) => ({
  label: link.label,
  href: link.to,
}));

function isModifiedClick(e: React.MouseEvent): boolean {
  return e.metaKey || e.ctrlKey || e.shiftKey || e.altKey;
}

const MOCK_STATS = {
  tps: 0,
  ledger: 0,
  accounts: 0,
  contracts: 0,
};

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
  const [network, setNetwork] = useState<Network>('mainnet');
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
      void navigate(routes.search(q));
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
        network={network}
        onNetworkChange={setNetwork}
        stats={MOCK_STATS}
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
        sx={{ flex: 1, ...(isFullBleed ? {} : { px: 10, py: 4 }) }}
      >
        <Outlet />
      </Box>
      <Footer
        logo={<HomeLogo height={47} onClick={handleHomeClick} />}
        navItems={FOOTER_NAV_ITEMS}
        network={network}
      />
    </Box>
  );
}
