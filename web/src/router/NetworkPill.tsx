import { useState } from 'react';
import Box from '@mui/material/Box';
import Menu from '@mui/material/Menu';
import MenuItem from '@mui/material/MenuItem';
import Typography from '@mui/material/Typography';

import { NETWORK_SITES, type Network } from '../network.js';

/**
 * The network this site shows, beside the logo, and the way to the other one
 * (task 0553). On testnet it is a filled brand-yellow pill, so no testnet page
 * passes for mainnet; on mainnet it is a quiet outline. Its menu names each
 * network with one line on what it is and links to the other deployment.
 */
export function NetworkPill({ current }: { current: Network }) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const isTestnet = current === 'testnet';

  return (
    <>
      <Box
        component="button"
        type="button"
        aria-haspopup="menu"
        aria-expanded={anchor !== null}
        aria-label={`Network: ${isTestnet ? 'Testnet' : 'Mainnet'}`}
        onClick={(e: React.MouseEvent<HTMLElement>) =>
          setAnchor(e.currentTarget)
        }
        sx={(theme) => ({
          display: 'inline-flex',
          alignItems: 'center',
          gap: 0.75,
          height: 28,
          px: 1.25,
          borderRadius: '8px',
          cursor: 'pointer',
          font: 'inherit',
          fontSize: 12,
          fontWeight: 700,
          letterSpacing: '0.06em',
          textTransform: 'uppercase',
          '&:focus-visible': {
            outline: `2px solid ${theme.palette.stroke.action}`,
            outlineOffset: 2,
          },
          ...(isTestnet
            ? {
                border: 'none',
                backgroundColor: theme.palette.surface.primaryMain,
                color: theme.palette.common.black,
                '&:hover': {
                  backgroundColor: theme.palette.surface.primaryHover,
                },
              }
            : {
                border: `1px solid ${theme.palette.stroke.default}`,
                backgroundColor: 'transparent',
                color: theme.palette.text.secondary,
                '&:hover': { borderColor: theme.palette.stroke.defaultHover },
              }),
        })}
      >
        {isTestnet ? 'Testnet' : 'Mainnet'}
        <Box
          component="span"
          aria-hidden
          sx={{
            fontSize: 9,
            transform: anchor ? 'rotate(180deg)' : 'none',
            transition: 'transform .15s',
          }}
        >
          ▼
        </Box>
      </Box>
      <Menu
        anchorEl={anchor}
        open={anchor !== null}
        onClose={() => setAnchor(null)}
        slotProps={{
          paper: { sx: { mt: 1, width: 300, borderRadius: '12px', p: 0.5 } },
          list: { sx: { py: 0 } },
        }}
      >
        <Typography
          variant="bodySmMedium"
          color="text.tertiary"
          sx={{ display: 'block', px: 1.5, pt: 1, pb: 0.5 }}
        >
          Network
        </Typography>
        {NETWORK_SITES.map((site) => {
          const isCurrent = site.key === current;
          return (
            <MenuItem
              key={site.key}
              component="a"
              href={site.url}
              selected={isCurrent}
              aria-current={isCurrent ? 'page' : undefined}
              sx={{
                alignItems: 'flex-start',
                gap: 1.25,
                borderRadius: '8px',
                py: 1,
                px: 1.5,
                whiteSpace: 'normal',
              }}
            >
              <Box
                component="span"
                aria-hidden
                sx={(theme) => ({
                  mt: '6px',
                  width: 10,
                  height: 10,
                  borderRadius: '50%',
                  flexShrink: 0,
                  backgroundColor:
                    site.key === 'testnet'
                      ? theme.palette.surface.primaryMain
                      : theme.palette.stroke.success,
                })}
              />
              <Box sx={{ flex: 1 }}>
                <Typography variant="bodySmSemiBold" component="div">
                  {site.name}
                </Typography>
                <Typography
                  variant="bodySmRegular"
                  color="text.secondary"
                  component="div"
                >
                  {site.about}
                </Typography>
              </Box>
              {isCurrent && (
                <Box component="span" aria-hidden sx={{ fontWeight: 700 }}>
                  ✓
                </Box>
              )}
            </MenuItem>
          );
        })}
      </Menu>
    </>
  );
}
