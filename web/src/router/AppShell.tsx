import { useState } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';

import {
  TopNav,
  SecondaryNav,
  Footer,
  type Network,
  type NavItem,
} from '@rumblefish/soroban-block-explorer-ui';

import { NAV_LINKS, routes } from './routes.js';

const NAV_ITEMS: NavItem[] = NAV_LINKS.map((link) => ({
  label: link.label,
  href: link.to,
}));

const MOCK_STATS = {
  tps: 0,
  ledger: 0,
  accounts: 0,
  contracts: 0,
};

const LogoSm = () => (
  <img
    src="/rumblefish-logo.svg"
    alt="Rumblefish"
    style={{ height: 32, display: 'block' }}
  />
);

const LogoLg = () => (
  <img
    src="/rumblefish-logo.svg"
    alt="Rumblefish"
    style={{ height: 47, display: 'block' }}
  />
);

function TestnetBanner() {
  return (
    <Box
      role="alert"
      aria-label="Testnet environment"
      sx={(theme) => ({
        backgroundColor: theme.palette.surface.warning,
        textAlign: 'center',
        py: 0.5,
        px: 2,
      })}
    >
      <Typography variant="bodyXsMedium" color="text.warning">
        TESTNET — not production data
      </Typography>
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
  const [network, setNetwork] = useState<Network>('mainnet');
  const [searchValue, setSearchValue] = useState('');

  const handleSearchSubmit = () => {
    const q = searchValue.trim();
    if (q) void navigate(routes.search(q));
  };

  const handleNavClick = (item: NavItem) => {
    if (item.href) void navigate(item.href);
  };

  return (
    <Box sx={{ minHeight: '100vh', display: 'flex', flexDirection: 'column' }}>
      {network === 'testnet' && <TestnetBanner />}
      <TopNav
        network={network}
        onNetworkChange={setNetwork}
        stats={MOCK_STATS}
        searchValue={searchValue}
        onSearchChange={setSearchValue}
        onSearchSubmit={handleSearchSubmit}
        onSearchClear={() => setSearchValue('')}
      />
      <SecondaryNav
        logo={<LogoSm />}
        navItems={NAV_ITEMS}
        activePage={activePage}
        onNavClick={handleNavClick}
      />
      <Box component="main" sx={{ flex: 1 }}>
        <Outlet />
      </Box>
      <Footer logo={<LogoLg />} navItems={NAV_ITEMS} network={network} />
    </Box>
  );
}
