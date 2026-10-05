import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';

import { NETWORK_SITES, type Network } from '../network.js';

/**
 * The network this site shows, and the way to the other one (task 0553): a
 * segmented control with one tab per network. The current network's tab is
 * highlighted; the other tab links to its deployment, since each network is
 * its own build on its own host.
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
              borderColor: isCurrent ? theme.palette.blue[600] : 'transparent',
              backgroundColor: isCurrent
                ? theme.palette.blue[100]
                : 'transparent',
              color: isCurrent
                ? theme.palette.blue[600]
                : theme.palette.text.secondary,
              transition: 'color 0.15s',
              '&:hover': isCurrent ? {} : { color: theme.palette.text.primary },
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
