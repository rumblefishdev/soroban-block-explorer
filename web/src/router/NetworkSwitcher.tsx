import { useState } from 'react';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import type { Theme } from '@mui/material/styles';
import { useLocation } from 'react-router-dom';

import { NETWORK_SITES, type Network } from '../network.js';
import { NAV_LINKS } from './routes.js';

/** From this width up the switcher sits beside the logo. Below it, on phones,
 *  the row has no room for it: the header shows only a TESTNET badge on
 *  testnet ({@link TestnetBadge}) and the choice moves into the menu drawer
 *  ({@link NetworkMenu}). */
export const SWITCHER_FROM = 'sm';

/** How long the highlight takes to slide to the other network. */
const SLIDE_MS = 220;

/**
 * The network this site shows, and the way to the other one (task 0553): both
 * network names side by side, the current one highlighted — blue for
 * mainnet, amber for testnet, so no testnet page passes for mainnet. The
 * whole control is one link to the other network: a click slides the
 * highlight across, then opens that network's site, since each network is
 * its own build on its own host. A modified click (new tab, new window) is
 * left to the browser.
 */
export function NetworkSwitcher({ current }: { current: Network }) {
  const section = sectionOf(useLocation().pathname);
  const [shown, setShown] = useState(current);
  const target = NETWORK_SITES.find((site) => site.key !== current);
  if (!target) return null;
  const href = `${target.url}${section}`;

  const handleClick = (e: React.MouseEvent<HTMLAnchorElement>) => {
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    e.preventDefault();
    setShown(target.key);
    const reduceMotion = window.matchMedia?.(
      '(prefers-reduced-motion: reduce)'
    ).matches;
    window.setTimeout(
      () => window.location.assign(href),
      reduceMotion ? 0 : SLIDE_MS
    );
  };

  return (
    <Box
      component="nav"
      aria-label="Network"
      sx={{ flexShrink: 0, display: { xs: 'none', [SWITCHER_FROM]: 'block' } }}
    >
      <Box
        component="a"
        href={href}
        onClick={handleClick}
        aria-label={`Network: ${nameOf(current)}. Switch to ${target.name}`}
        sx={switchShape}
      >
        <Box aria-hidden sx={[thumbShape, thumbLook(shown)]} />
        {NETWORK_SITES.map((site) => (
          <Typography
            key={site.key}
            aria-hidden
            variant="bodySmMedium"
            noWrap
            sx={[labelShape, labelLook(site.key, shown)]}
          >
            {site.name}
          </Typography>
        ))}
      </Box>
    </Box>
  );
}

const nameOf = (key: Network) =>
  NETWORK_SITES.find((site) => site.key === key)?.name ?? key;

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
// component. They come from the colour scales, which are the same in both
// themes: the highlight is a pastel chip on either background, so its ink
// must stay dark too.
export function tabColors(theme: Theme, key: Network) {
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

const slide = `${SLIDE_MS}ms cubic-bezier(.4, 0, .2, 1)`;

// The control: two equal columns, one per network, in a framed box.
const switchShape = (theme: Theme) => ({
  position: 'relative' as const,
  display: 'inline-grid',
  gridTemplateColumns: '1fr 1fr',
  p: 0.5,
  borderRadius: `${theme.shape.radius.s}px`,
  border: `1px solid ${theme.palette.stroke.default}`,
  backgroundColor: theme.palette.surface.grayMain,
  textDecoration: 'none',
  '&:hover': { borderColor: theme.palette.stroke.defaultHover },
  '&:focus-visible': {
    outline: `2px solid ${theme.palette.stroke.action}`,
    outlineOffset: 2,
  },
});

// The highlight: one box that sits under the current network's name and
// slides to the other half on a switch.
const thumbShape = (theme: Theme) => ({
  position: 'absolute' as const,
  top: 4,
  bottom: 4,
  left: 4,
  width: 'calc(50% - 4px)',
  borderRadius: `${theme.shape.radius.s}px`,
  border: '1px solid',
  transition: `transform ${slide}, background-color ${slide}, border-color ${slide}`,
  '@media (prefers-reduced-motion: reduce)': { transition: 'none' },
});

const thumbLook = (shown: Network) => (theme: Theme) => ({
  borderColor: tabColors(theme, shown).border,
  backgroundColor: tabColors(theme, shown).fill,
  transform: shown === 'testnet' ? 'translateX(100%)' : 'none',
});

const labelShape = {
  position: 'relative' as const,
  px: 1.5,
  py: 0.25,
  textAlign: 'center' as const,
  transition: `color ${slide}`,
};

const labelLook = (key: Network, shown: Network) => (theme: Theme) => ({
  color:
    key === shown ? tabColors(theme, key).text : theme.palette.text.secondary,
});
