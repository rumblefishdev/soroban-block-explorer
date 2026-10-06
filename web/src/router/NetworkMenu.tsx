import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import CheckIcon from '@mui/icons-material/Check';
import { useLocation } from 'react-router-dom';

import { NETWORK_SITES, type Network } from '../network.js';
import { SWITCHER_FROM, sectionOf } from './NetworkSwitcher.js';

/**
 * The network choice inside the menu drawer, on phones only — there the
 * header has no room for the switcher (task 0553). Both networks are menu
 * rows, the current one ticked; the other one is a plain link to its site,
 * landing on the same section as the switcher does.
 */
export function NetworkMenu({ current }: { current: Network }) {
  const section = sectionOf(useLocation().pathname);

  return (
    <Box
      component="nav"
      aria-label="Network"
      sx={(theme) => ({
        display: { xs: 'block', [SWITCHER_FROM]: 'none' },
        mt: 1,
        pt: 1,
        borderTop: `1px solid ${theme.palette.stroke.default}`,
      })}
    >
      <Typography
        variant="bodyXsMedium"
        color="text.tertiary"
        sx={{ display: 'block', px: 1, pb: 0.5 }}
      >
        Network
      </Typography>
      {NETWORK_SITES.map((site) =>
        site.key === current ? (
          <Box
            key={site.key}
            aria-current="true"
            sx={(theme) => ({
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              px: 1,
              py: 1,
              color: theme.palette.text.primary,
            })}
          >
            <Typography variant="bodyMedium" color="inherit">
              {site.name}
            </Typography>
            <CheckIcon aria-hidden sx={{ fontSize: 18 }} />
          </Box>
        ) : (
          <Box
            key={site.key}
            component="a"
            href={`${site.url}${section}`}
            sx={(theme) => ({
              display: 'flex',
              px: 1,
              py: 1,
              textDecoration: 'none',
              color: theme.palette.text.tertiary,
              transition: 'background-color 0.15s, color 0.15s',
              '&:hover': {
                backgroundColor: theme.palette.surface.background,
                borderRadius: `${theme.shape.radius.s}px`,
                color: theme.palette.text.secondary,
              },
            })}
          >
            <Typography variant="bodyMedium" color="inherit">
              {site.name}
            </Typography>
          </Box>
        )
      )}
    </Box>
  );
}
