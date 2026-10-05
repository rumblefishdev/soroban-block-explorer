import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import type { Theme } from '@mui/material/styles';

import { NETWORK_SITES, type Network } from '../network.js';

/**
 * The network this site shows, and the way to the other one (task 0553): a
 * segmented control with one tab per network. The current network's tab is
 * highlighted — blue for mainnet, amber for testnet, so no testnet page
 * passes for mainnet; the other tab links to its deployment, since each
 * network is its own build on its own host.
 */
export function NetworkSwitcher({ current }: { current: Network }) {
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
            href={site.url}
            aria-current={isCurrent ? 'page' : undefined}
            sx={(theme) => ({
              display: 'inline-flex',
              alignItems: 'center',
              px: 1.5,
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
            <Typography variant="bodySmMedium" color="inherit" noWrap>
              {site.name}
            </Typography>
          </Box>
        );
      })}
    </Box>
  );
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
