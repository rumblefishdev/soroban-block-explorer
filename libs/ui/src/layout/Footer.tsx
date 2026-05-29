import type { ReactNode } from 'react';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';

import { grid } from '../theme/grid.js';

export interface FooterNavItem {
  label: string;
  href?: string;
  onClick?: (e: React.MouseEvent<HTMLAnchorElement>) => void;
}

export interface FooterProps {
  logo: ReactNode;
  navItems: FooterNavItem[];
}

const RESOURCES: FooterNavItem[] = [
  { label: 'GitHub' },
  { label: 'Stellar docs' },
  { label: 'Soroban docs' },
  { label: 'Stellar dashboard' },
];

const LEGAL: FooterNavItem[] = [
  { label: 'Terms of Service' },
  { label: 'Privacy Policy' },
  { label: 'Cookies' },
];

function FooterLink({ label, href, onClick }: FooterNavItem) {
  return (
    <Box
      component={href ? 'a' : 'span'}
      {...(href ? { href } : {})}
      {...(onClick ? { onClick } : {})}
      sx={(theme) => ({
        px: 1,
        py: 0.5,
        ...theme.typography.bodySmMedium,
        color: theme.palette.text.tertiary,
        textDecoration: 'none',
        display: 'inline-block',
        '&:hover': href ? { color: theme.palette.text.primary } : {},
      })}
    >
      {label}
    </Box>
  );
}

export function Footer({ logo, navItems }: FooterProps) {
  return (
    <Box
      component="footer"
      sx={(theme) => ({
        width: '100%',
        backgroundColor: theme.palette.surface.backgroundAlt,
        borderTop: `1px solid ${theme.palette.stroke.default}`,
        // Sit above the page grid backdrop on every route so the grid
        // lines never bleed through the footer.
        position: 'relative',
        zIndex: 2,
      })}
    >
      <Box
        sx={{
          width: '100%',
          maxWidth: grid.desktop.maxWidth,
          mx: 'auto',
          px: {
            xs: `${grid.mobile.margin}px`,
            md: `${grid.desktop.margin}px`,
          },
          pt: { xs: 5, md: 10 },
          pb: 5,
          display: 'flex',
          flexDirection: 'column',
          gap: 3,
        }}
      >
        <Box
          sx={{
            display: 'flex',
            alignItems: 'flex-start',
            justifyContent: 'space-between',
            flexDirection: { xs: 'column', md: 'row' },
            gap: { xs: 4, md: 2 },
          }}
        >
          {/* Left: logo + description + status */}
          <Box
            display="flex"
            flexDirection="column"
            gap={3}
            sx={{ width: { xs: '100%', md: 283 } }}
          >
            <Box sx={{ display: 'flex', justifyContent: 'left' }}>{logo}</Box>
            <Box display="flex" flexDirection="column" gap={1}>
              <Typography variant="bodySmMedium" color="text.tertiary">
                A block explorer for the Stellar and Soroban network. Browse
                transactions, accounts, contracts and more.
              </Typography>
              <Box
                sx={(theme) => ({
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: 1,
                  px: 1,
                  py: 0.25,
                  borderRadius: '8px',
                  backgroundColor: theme.palette.surface.success,
                  alignSelf: 'flex-start',
                })}
              >
                <Box
                  sx={(theme) => ({
                    width: 8,
                    height: 8,
                    borderRadius: '50%',
                    backgroundColor: theme.palette.text.success,
                    flexShrink: 0,
                  })}
                />
                <Typography variant="bodySmMedium" color="text.success" noWrap>
                  All systems operational
                </Typography>
              </Box>
            </Box>
          </Box>

          {/* Middle: Explorer nav links */}
          <Box
            display="flex"
            flexDirection="column"
            gap={1}
            sx={{ width: { xs: '100%', md: 320 } }}
          >
            <Typography variant="heading6SemiBold" color="text.primary">
              Explorer
            </Typography>
            <Box display="flex" flexWrap="wrap" gap={1}>
              {navItems.map((item) => (
                <FooterLink key={item.label} {...item} />
              ))}
            </Box>
          </Box>

          {/* Right: Resources */}
          <Box
            display="flex"
            flexDirection="column"
            gap={1}
            sx={{ width: { xs: '100%', md: 244 } }}
          >
            <Typography variant="heading6SemiBold" color="text.primary">
              Resources
            </Typography>
            <Box display="flex" flexWrap="wrap" gap={1}>
              {RESOURCES.map((item) => (
                <FooterLink key={item.label} {...item} />
              ))}
            </Box>
          </Box>
        </Box>

        {/* Divider */}
        <Box
          sx={(theme) => ({
            width: '100%',
            height: '1px',
            backgroundColor: theme.palette.stroke.default,
            flexShrink: 0,
          })}
        />

        {/* Bottom: copyright + legal + network badge */}
        <Box
          sx={{
            display: 'flex',
            alignItems: { xs: 'flex-start', md: 'center' },
            flexDirection: { xs: 'column', md: 'row' },
            gap: { xs: 1, md: 4 },
          }}
        >
          <Box flex={1} minWidth={0}>
            <Typography
              variant="bodySmMedium"
              color="text.tertiary"
              sx={{ whiteSpace: { xs: 'normal', md: 'nowrap' } }}
            >
              © {new Date().getFullYear()} Stellar Explorer. Built on the
              Stellar network.
            </Typography>
          </Box>
          <Box display="flex" alignItems="center" flexWrap="wrap">
            {LEGAL.map((item) => (
              <FooterLink key={item.label} {...item} />
            ))}
          </Box>
        </Box>
      </Box>
    </Box>
  );
}
