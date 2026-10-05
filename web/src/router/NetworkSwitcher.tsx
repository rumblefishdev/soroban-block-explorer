import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import type { Theme } from '@mui/material/styles';
import { useLocation } from 'react-router-dom';

import { NETWORK_SITES, type Network } from '../network.js';
import { NAV_LINKS } from './routes.js';

/**
 * The network this site shows, and the way to the other one (task 0553): a
 * segmented control with one tab per network. The current network's tab is
 * highlighted — blue for mainnet, amber for testnet, so no testnet page
 * passes for mainnet; the other tab links to its deployment, since each
 * network is its own build on its own host.
 */
export function NetworkSwitcher({ current }: { current: Network }) {
  const section = sectionOf(useLocation().pathname);
  return (
    <Box
      component="nav"
      aria-label="Network"
      sx={(theme) => ({
        display: 'inline-flex',
        alignItems: 'center',
        flexShrink: 0,
        p: 0.5,
        borderRadius: `${theme.shape.radius.s}px`,
        border: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.grayMain,
      })}
    >
      {NETWORK_SITES.map((site) => {
        const isCurrent = site.key === current;
        return (
          <Box
            key={site.key}
            component="a"
            href={`${site.url}${section}`}
            aria-current={isCurrent ? 'page' : undefined}
            sx={(theme) => ({
              display: 'inline-flex',
              alignItems: 'center',
              // Narrower on phones, so logo, switcher, theme toggle and menu
              // fit one row at 360px.
              px: { xs: 0.75, sm: 1.5 },
              py: 0.25,
              borderRadius: `${theme.shape.radius.s}px`,
              textDecoration: 'none',
              border: '1px solid',
              borderColor: isCurrent
                ? tabColors(theme, site.key).border
                : 'transparent',
              backgroundColor: isCurrent
                ? tabColors(theme, site.key).fill
                : 'transparent',
              color: isCurrent
                ? tabColors(theme, site.key).text
                : theme.palette.text.secondary,
              transition: 'background-color 0.15s',
              '&:hover': isCurrent
                ? {}
                : { backgroundColor: tabColors(theme, site.key).fill },
              '&:focus-visible': {
                outline: `2px solid ${theme.palette.stroke.action}`,
                outlineOffset: 2,
              },
            })}
          >
            <Typography
              variant="bodySmMedium"
              color="inherit"
              noWrap
              sx={{ fontSize: { xs: 12, sm: 14 } }}
            >
              {site.name}
            </Typography>
          </Box>
        );
      })}
    </Box>
  );
}

/** Where a switch lands on the other network: the list of the section you
 *  are in, or home. A transaction, a ledger or a contract is not the same
 *  thing on two networks — the same hash or sequence there is another record
 *  or none at all — so a detail page lands on its section's list, and search
 *  and other pages on home. */
export function sectionOf(pathname: string): string {
  const link = NAV_LINKS.find(
    (l) => pathname === l.to || pathname.startsWith(`${l.to}/`)
  );
  return link ? link.to : '';
}

// Each network's colours, from the design's "Mainnet / Testnet tabs"
// component: the current tab takes all three, a hovered other tab only the
// fill.
function tabColors(theme: Theme, key: Network) {
  if (key === 'testnet') {
    return {
      fill: theme.palette.yellow[100],
      border: theme.palette.yellow[500],
      text: theme.palette.yellow[700],
    };
  }
  return {
    fill: theme.palette.blue[100],
    border: theme.palette.blue[600],
    text: theme.palette.blue[600],
  };
}
